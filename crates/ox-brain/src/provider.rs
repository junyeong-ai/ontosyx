use std::time::Duration;

use ox_core::error::{OxError, OxResult};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Shared types — used by Brain trait methods and callers
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamChunk {
    pub delta: String,
    pub is_final: bool,
    pub usage: Option<TokenUsage>,
}

// ---------------------------------------------------------------------------
// structured_completion — uses branchforge Client for LLM calls
// ---------------------------------------------------------------------------

/// Parse the LLM response as JSON into a typed struct.
///
/// Uses branchforge `Client.send()` with JSON Schema enforcement.
/// Applies ox-brain schema transformations for Bedrock/Anthropic compatibility.
/// Schema complexity thresholds for structured output.
/// When a JSON Schema exceeds these limits, falls back to plain JSON mode.
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

pub async fn structured_completion<T: serde::de::DeserializeOwned + schemars::JsonSchema>(
    client: &branchforge::Client,
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
    client: &branchforge::Client,
    model: &str,
    system: &str,
    user_prompt: &str,
    max_tokens: u32,
    temperature: Option<f32>,
    thresholds: SchemaComplexityThresholds,
) -> OxResult<T> {
    use branchforge::client::{CreateMessageRequest, OutputFormat};
    use branchforge::types::{CacheTtl, Message, SystemBlock, SystemPrompt};

    let max_optional = thresholds.max_optional_params;
    let max_total = thresholds.max_total_properties;

    // Generate JSON Schema from the Rust type
    let schema = schemars::schema_for!(T);
    let mut schema_value = schema.to_value();

    let type_name = schema_value
        .get("title")
        .and_then(|t| t.as_str())
        .unwrap_or("response")
        .to_string();

    // Transform schema for provider compatibility (Bedrock/Anthropic don't support oneOf/$ref)
    schema_value = crate::schema::transform_for_structured_output(&schema_value);
    crate::schema::enforce_strict_object_schemas(&mut schema_value);
    crate::schema::clean_nullable_flags(&mut schema_value);

    // Check complexity — fall back to JSON-only prompt if too complex
    let optional_count = crate::schema::count_optional_params(&schema_value);
    let total_props = crate::schema::count_total_properties(&schema_value);

    let use_schema = optional_count <= max_optional && total_props <= max_total;

    // Prompt caching: system prompt cached with 1-hour TTL.
    // Repeated calls with the same system prompt hit Anthropic's prompt cache,
    // reducing input token costs significantly.
    let cached_system = SystemPrompt::Blocks(vec![SystemBlock::cached_with_ttl(
        system,
        CacheTtl::OneHour,
    )]);

    let mut request = CreateMessageRequest::new(model, vec![Message::user(user_prompt)])
        .max_tokens(max_tokens)
        .system(cached_system);

    if let Some(temp) = temperature {
        request = request.temperature(temp);
    }

    if use_schema {
        request = request.output_format(OutputFormat::json_schema(schema_value));
        tracing::debug!(schema_name = %type_name, "Using structured output with JSON Schema");
    } else {
        tracing::info!(
            optional_count,
            total_props,
            "Schema too complex for structured output, using JSON mode"
        );
    }

    // Send with transport-level retry (rate limit, overloaded, 5xx)
    let response = match send_with_retry(client, request.clone()).await {
        Ok(resp) => resp,
        // Content filter with schema → mode switch (not a retry, different request)
        Err(e) if use_schema && is_content_filtered(&e) => {
            tracing::warn!(
                "Content filtering blocked structured output, falling back to JSON mode"
            );
            let mut fallback = request;
            fallback.output_format = None;
            send_with_retry(client, fallback)
                .await
                .map_err(|e| OxError::Runtime {
                    message: format!("LLM request failed (JSON fallback): {e}"),
                })?
        }
        Err(e) => {
            return Err(OxError::Runtime {
                message: format!("LLM request failed: {e}"),
            });
        }
    };

    let content = response.text();

    if response.stop_reason == Some(branchforge::types::StopReason::MaxTokens) {
        return Err(OxError::Runtime {
            message: format!(
                "LLM output truncated (max_tokens reached). Output length: {} chars",
                content.len()
            ),
        });
    }

    let json_str = extract_json(&content);
    serde_json::from_str(json_str).map_err(|e| OxError::Runtime {
        message: format!("Failed to parse structured output: {e}\nRaw: {json_str}"),
    })
}

/// Extract JSON from a response that might be wrapped in ```json ... ```
fn extract_json(text: &str) -> &str {
    let trimmed = text.trim();
    // Try ```json ... ``` (with closing fence)
    if let Some(start) = trimmed.find("```json") {
        let json_start = start + 7;
        if let Some(end) = trimmed[json_start..].find("```") {
            return trimmed[json_start..json_start + end].trim();
        }
        // No closing fence (truncated output) — strip opening fence only
        return trimmed[json_start..].trim();
    }
    // Try ``` ... ``` (generic code fence)
    if let Some(start) = trimmed.find("```") {
        let json_start = start + 3;
        let json_start = trimmed[json_start..]
            .find('\n')
            .map(|n| json_start + n + 1)
            .unwrap_or(json_start);
        if let Some(end) = trimmed[json_start..].find("```") {
            return trimmed[json_start..json_start + end].trim();
        }
        // No closing fence — strip opening fence only
        return trimmed[json_start..].trim();
    }
    trimmed
}

/// Send a request with transport-level retry for transient errors.
/// Handles rate limits, overloaded models, circuit breaker, and 5xx errors.
async fn send_with_retry(
    client: &branchforge::Client,
    request: branchforge::client::CreateMessageRequest,
) -> Result<branchforge::types::ApiResponse, branchforge::Error> {
    const MAX_RETRIES: u32 = 2;

    let mut last_error = None;
    for attempt in 0..=MAX_RETRIES {
        match client.send(request.clone()).await {
            Ok(resp) => return Ok(resp),
            Err(e) if attempt < MAX_RETRIES && is_retryable(&e) => {
                let delay = Duration::from_millis(500 * 2u64.pow(attempt));
                tracing::warn!(
                    attempt = attempt + 1,
                    "Retryable LLM error, retrying in {delay:?}: {e}"
                );
                tokio::time::sleep(delay).await;
                last_error = Some(e);
            }
            Err(e) => return Err(e),
        }
    }
    Err(last_error.unwrap_or_else(|| branchforge::Error::Api {
        message: "retry loop completed without any attempt".into(),
        status: None,
        error_type: None,
    }))
}

fn is_retryable(err: &branchforge::Error) -> bool {
    matches!(
        err,
        branchforge::Error::RateLimit { .. }
            | branchforge::Error::ModelOverloaded { .. }
            | branchforge::Error::CircuitOpen
    ) || matches!(
        err,
        branchforge::Error::Api {
            status: Some(500..=599),
            ..
        }
    )
}

fn is_content_filtered(err: &branchforge::Error) -> bool {
    match err {
        branchforge::Error::Api { message, .. } => {
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
}
