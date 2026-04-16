use branchforge::ir::{JsonSchemaSpec, ResponseFormat};
use branchforge::{FinishReason, LlmCall, Message, ModelRequest, ModelResponse, SystemBlock, SystemPrompt};
use ox_core::error::{OxError, OxResult};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Shared types — used by Brain trait methods and callers
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamChunk {
    pub delta: String,
    pub is_final: bool,
    pub usage: Option<TokenUsage>,
}

// ---------------------------------------------------------------------------
// structured_completion — uses branchforge ProviderClient for LLM calls
// ---------------------------------------------------------------------------

/// Schema complexity thresholds for structured output quality.
/// When a JSON Schema exceeds these limits, falls back to plain JSON mode
/// because LLMs struggle to produce valid output for very complex schemas.
#[derive(Debug, Clone, Copy)]
pub struct SchemaComplexityThresholds {
    pub max_optional_params: usize,
    pub max_total_properties: usize,
}

impl Default for SchemaComplexityThresholds {
    fn default() -> Self {
        Self {
            max_optional_params: 24,
            max_total_properties: 50,
        }
    }
}

/// Parse the LLM response as JSON into a typed struct.
///
/// Uses branchforge `ProviderClient.send()` with native JSON Schema enforcement.
/// branchforge 0.9 handles all provider-specific schema transformations internally
/// (Anthropic, Bedrock, OpenAI, Gemini) — no manual pre-processing needed.
pub async fn structured_completion<T: serde::de::DeserializeOwned + schemars::JsonSchema>(
    client: &dyn LlmCall,
    model: &str,
    system: &str,
    user_prompt: &str,
    max_tokens: u32,
    temperature: Option<f32>,
) -> OxResult<T> {
    structured_completion_with_thresholds(
        client,
        model,
        system,
        user_prompt,
        max_tokens,
        temperature,
        SchemaComplexityThresholds::default(),
    )
    .await
}

pub async fn structured_completion_with_thresholds<
    T: serde::de::DeserializeOwned + schemars::JsonSchema,
>(
    client: &dyn LlmCall,
    model: &str,
    system: &str,
    user_prompt: &str,
    max_tokens: u32,
    temperature: Option<f32>,
    thresholds: SchemaComplexityThresholds,
) -> OxResult<T> {
    let max_optional = thresholds.max_optional_params;
    let max_total = thresholds.max_total_properties;

    // Generate JsonSchemaSpec from the Rust type.
    // branchforge 0.9 handles all provider-specific schema transformations
    // (inlining $ref, enforcing additionalProperties:false, stripping
    // unsupported keywords) via the codec-level SchemaPolicy pipeline.
    let spec = JsonSchemaSpec::from_type::<T>();

    let type_name = spec
        .schema
        .get("title")
        .and_then(|t| t.as_str())
        .unwrap_or("response")
        .to_string();

    // Check complexity — fall back to JSON-only prompt if too complex
    let optional_count = crate::schema::count_optional_params(&spec.schema);
    let total_props = crate::schema::count_total_properties(&spec.schema);

    let use_schema = optional_count <= max_optional && total_props <= max_total;

    // When falling back to JSON-only mode (no schema enforcement), append
    // an explicit JSON-only instruction to prevent the LLM from outputting
    // reasoning text before the JSON object.
    let effective_system = if use_schema {
        system.to_string()
    } else {
        format!(
            "{system}\n\n\
             CRITICAL: You MUST output ONLY a valid JSON object. \
             Do NOT include any explanation, reasoning, or text before or after the JSON. \
             Start your response with {{ and end with }}."
        )
    };

    // Prompt caching: system prompt cached with 1-hour TTL.
    let cached_system = SystemPrompt::Blocks(vec![SystemBlock::cached_with_ttl(
        &effective_system,
        "1h",
    )]);

    let mut request = ModelRequest::new(model, vec![Message::user(user_prompt)])
        .with_max_tokens(max_tokens)
        .with_system(cached_system);

    if let Some(temp) = temperature {
        request = request.with_temperature(temp);
    }

    if use_schema {
        request = request.with_response_format(ResponseFormat::JsonSchema(spec));
        tracing::debug!(schema_name = %type_name, "Using structured output with JSON Schema");
    } else {
        tracing::info!(
            optional_count,
            total_props,
            "Schema too complex for structured output, using JSON mode"
        );
    }

    // branchforge ProviderClient handles retry/backoff internally.
    let (response, schema_enforced) = match client.send(&request).await {
        Ok(resp) => (resp, use_schema),
        // Content filter with schema → rebuild request in JSON-only mode.
        // Must update BOTH response_format AND system prompt so the LLM
        // receives the explicit JSON-only instruction.
        Err(e) if use_schema && is_content_filtered(&e) => {
            tracing::warn!(
                "Content filtering blocked structured output, falling back to JSON mode"
            );
            request.response_format = None;
            request = request.with_system(SystemPrompt::Blocks(vec![SystemBlock::cached_with_ttl(
                format!(
                    "{system}\n\n\
                     CRITICAL: You MUST output ONLY a valid JSON object. \
                     Do NOT include any explanation, reasoning, or text before or after the JSON. \
                     Start your response with {{ and end with }}."
                ),
                "1h",
            )]));
            let resp = client.send(&request).await.map_err(|e| OxError::Runtime {
                message: format!("LLM request failed (JSON fallback): {e}"),
            })?;
            (resp, false) // Schema no longer enforced by provider
        }
        Err(e) => {
            return Err(OxError::Runtime {
                message: format!("LLM request failed: {e}"),
            });
        }
    };

    // When provider-level schema enforcement was active, use branchforge's
    // finish-reason-aware JSON parser (handles ContentFilter, Length, parse errors).
    if schema_enforced {
        return response.json::<T>().map_err(|e| OxError::Runtime {
            message: format!("Failed to parse structured output: {e}"),
        });
    }

    // JSON-only mode: manual extraction with fallback for LLM self-corrections.
    parse_json_response::<T>(&response)
}

/// Parse a ModelResponse as JSON, handling common LLM output patterns.
fn parse_json_response<T: serde::de::DeserializeOwned>(response: &ModelResponse) -> OxResult<T> {
    let content = response.text();

    if response.finish_reason == FinishReason::Length {
        return Err(OxError::Runtime {
            message: format!(
                "LLM output truncated (max_tokens reached). Output length: {} chars",
                content.len()
            ),
        });
    }

    let json_str = extract_json(&content);
    serde_json::from_str(json_str)
        .or_else(|first_err| {
            // LLM self-correction: output contains multiple JSON objects
            // (e.g., first attempt + "Wait, let me fix..." + corrected attempt).
            // Try parsing the last complete JSON object.
            if let Some(extracted) = extract_last_json(&content) {
                serde_json::from_str(extracted).map_err(|_| first_err)
            } else {
                Err(first_err)
            }
        })
        .map_err(|e| OxError::Runtime {
            message: format!("Failed to parse structured output: {e}\nRaw: {json_str}"),
        })
}

/// Extract the last complete JSON object from a multi-JSON response.
///
/// When the LLM self-corrects ("Wait, let me fix..."), it produces multiple JSON
/// objects. This function finds the last balanced `{...}` block and returns it.
/// Returns `None` if no balanced JSON object is found.
fn extract_last_json(text: &str) -> Option<&str> {
    let bytes = text.as_bytes();
    let mut depth = 0i32;
    let mut end = None;

    // Scan from the end to find the last '}'
    for i in (0..bytes.len()).rev() {
        if bytes[i] == b'}' && end.is_none() {
            end = Some(i);
            depth = 1;
        } else if bytes[i] == b'}' && end.is_some() {
            depth += 1;
        } else if bytes[i] == b'{' && end.is_some() {
            depth -= 1;
            if depth == 0 {
                // end is Some here per the else-if guard; let-else silences
                // clippy without adding a runtime panic path.
                let Some(end_idx) = end else { continue };
                let candidate = &text[i..=end_idx];
                // Verify it's valid JSON
                if serde_json::from_str::<serde_json::Value>(candidate).is_ok() {
                    return Some(candidate);
                }
                // Not valid — keep scanning
                end = None;
            }
        }
    }
    None
}

/// Extract JSON from a response that might contain reasoning text before the JSON.
///
/// Handles three common LLM output patterns:
/// 1. ```json ... ``` (code fence wrapped)
/// 2. "Some reasoning text...\n{...}" (reasoning prefix)
/// 3. Plain JSON
fn extract_json(text: &str) -> &str {
    let trimmed = text.trim();

    // Pattern 1: ```json ... ``` (with closing fence)
    if let Some(start) = trimmed.find("```json") {
        let json_start = start + 7;
        if let Some(end) = trimmed[json_start..].find("```") {
            return trimmed[json_start..json_start + end].trim();
        }
        return trimmed[json_start..].trim();
    }

    // Pattern 1b: ``` ... ``` (generic code fence)
    if let Some(start) = trimmed.find("```") {
        let json_start = start + 3;
        let json_start = trimmed[json_start..]
            .find('\n')
            .map(|n| json_start + n + 1)
            .unwrap_or(json_start);
        if let Some(end) = trimmed[json_start..].find("```") {
            return trimmed[json_start..json_start + end].trim();
        }
        return trimmed[json_start..].trim();
    }

    // Pattern 2: Reasoning text followed by JSON object
    // LLM outputs "Looking at the ontology...\n\n{"operation": ...}"
    // Find a '{' that starts a line (after newline or start) — avoids
    // false matches on inline braces like "{Product}" in prose.
    if !trimmed.starts_with('{')
        && !trimmed.starts_with('[')
        && let Some(last_brace) = trimmed.rfind('}')
    {
        // Scan for a '{' that is either at line start or preceded by whitespace/newline
        let bytes = trimmed.as_bytes();
        for (i, &b) in bytes.iter().enumerate() {
            if b == b'{' && i < last_brace {
                let at_line_start = i == 0 || bytes[i - 1] == b'\n' || bytes[i - 1] == b'\r';
                if at_line_start {
                    return &trimmed[i..=last_brace];
                }
            }
        }
    }

    // Pattern 3: Plain JSON (already valid)
    trimmed
}

fn is_content_filtered(err: &branchforge::Error) -> bool {
    match err {
        branchforge::Error::Provider { message, .. } => {
            message.contains("content filter") || message.contains("guardrail")
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_json_plain() {
        let input = r#"{"name": "test"}"#;
        assert_eq!(extract_json(input), r#"{"name": "test"}"#);
    }

    #[test]
    fn test_extract_json_code_block() {
        let input = "Here is the result:\n```json\n{\"name\": \"test\"}\n```\nDone.";
        assert_eq!(extract_json(input), r#"{"name": "test"}"#);
    }

    #[test]
    fn test_extract_json_generic_code_block() {
        let input = "```\n{\"key\": 42}\n```";
        assert_eq!(extract_json(input), r#"{"key": 42}"#);
    }

    #[test]
    fn test_extract_last_json_self_correction() {
        // LLM outputs first JSON, then self-corrects with a second
        let input = r#"{"op": "match", "bad": true}

Wait, I need to fix the query.

{"op": "chain", "steps": [{"pass_through": []}]}"#;
        let result = extract_last_json(input);
        assert!(result.is_some());
        let parsed: serde_json::Value = serde_json::from_str(result.unwrap()).unwrap();
        assert_eq!(parsed["op"], "chain");
    }

    #[test]
    fn test_extract_last_json_single_object() {
        // Single JSON — should still work
        let input = r#"{"name": "test"}"#;
        let result = extract_last_json(input);
        assert!(result.is_some());
        assert_eq!(result.unwrap(), r#"{"name": "test"}"#);
    }

    #[test]
    fn test_extract_last_json_no_json() {
        let input = "No JSON here at all";
        assert!(extract_last_json(input).is_none());
    }
}
