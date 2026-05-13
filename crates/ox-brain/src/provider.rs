//! `structured_completion` — typed JSON dispatch through a
//! [`ChatRunnable`].
//!
//! Builds a fully-formed [`entelix::ir::ModelRequest`] with native
//! structured-output enforcement (`ResponseFormat::strict`) and a
//! one-hour-cached system prompt; dispatches through the model's
//! codec + transport pipeline; parses the response as `T`.
//!
//! entelix's codec layer produces vendor-canonical structured-output
//! payloads (Anthropic `output_config.format`, OpenAI
//! `response_format`, Gemini `responseJsonSchema`) and
//! `ChatModelConfig::validation_retries` handles parse-failure
//! retries inside the model when callers route through
//! `ChatModel::complete_typed::<T>`. Brain operations want both the
//! parsed payload **and** the token-usage tuple, so they stay on
//! `complete_request` and parse manually.

use entelix::ExecutionContext;
use entelix::ir::{
    CacheControl, JsonSchemaSpec, Message, ModelRequest, ModelResponse, ResponseFormat, StopReason,
    SystemPrompt,
};
use ox_core::error::{OxError, OxResult};
use serde::{Deserialize, Serialize};

use crate::chat_model::ChatRunnable;
use crate::entelix_error::map_entelix_err;

/// Token consumption recorded from one structured-completion dispatch.
///
/// Mirrors [`entelix::ir::Usage`] for every axis ontosyx persists into
/// [`ox_ontology::ModelCall`] and the OTel GenAI span:
///
/// - `cached_input_tokens` — prompt-cache **read** count, non-zero when
///   the codec hit the `SystemPrompt::cached` breakpoint;
/// - `cache_creation_input_tokens` — prompt-cache **write** count, billed
///   at the per-million premium that establishes a new breakpoint;
/// - `reasoning_tokens` — extended-thinking tokens (Anthropic
///   `thinking`, OpenAI o-series internal reasoning), billed at the
///   output rate per provider convention.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_input_tokens: u64,
    pub cache_creation_input_tokens: u64,
    pub reasoning_tokens: u64,
}

/// Dispatch a structured-output completion and parse the response as `T`.
///
/// `system` rides on a one-hour cache breakpoint so Anthropic /
/// Bedrock-on-Claude codec paths emit the cache directive natively.
/// `temperature` is forwarded as-is when supplied; the model's
/// configured default applies when `None`.
///
/// **Internal only.** Every Brain LLM operation routes through
/// `DefaultBrain::call_structured_traced` /
/// `call_structured_traced_composed` so the budget gate / RunBudget
/// cost observation / OTel GenAI span / evaluation capture /
/// provenance pipeline run uniformly. Direct callers would bypass
/// the funnel — the `pub(crate)` ceiling closes that escape hatch.
pub(crate) async fn structured_completion<T>(
    chat: &ChatRunnable,
    model: &str,
    system: &str,
    user_prompt: &str,
    max_tokens: u32,
    temperature: Option<f32>,
    ctx: &ExecutionContext,
) -> OxResult<(T, TokenUsage)>
where
    T: serde::de::DeserializeOwned + schemars::JsonSchema,
{
    let schema_value =
        serde_json::to_value(schemars::schema_for!(T)).map_err(|e| OxError::Runtime {
            message: format!("schema generation failed for structured output: {e}"),
        })?;
    let type_name = std::any::type_name::<T>()
        .rsplit("::")
        .next()
        .unwrap_or("response");
    let spec = JsonSchemaSpec::new(type_name, schema_value)
        .map_err(map_entelix_err("LLM request failed"))?;

    let request = ModelRequest {
        model: model.to_owned(),
        messages: vec![Message::user(user_prompt)],
        system: SystemPrompt::cached(system, CacheControl::one_hour()),
        max_tokens: Some(max_tokens),
        temperature,
        response_format: Some(ResponseFormat::strict(spec)),
        ..ModelRequest::default()
    };

    let response = chat
        .complete_request(request, ctx)
        .await
        .map_err(map_entelix_err("LLM request failed"))?;

    // Record token consumption — the Prometheus recorder installed in
    // ox-api aggregates these counters for capacity planning.
    metrics::counter!("ox_brain.tokens.input").increment(u64::from(response.usage.input_tokens));
    metrics::counter!("ox_brain.tokens.output").increment(u64::from(response.usage.output_tokens));

    let usage = TokenUsage {
        input_tokens: u64::from(response.usage.input_tokens),
        output_tokens: u64::from(response.usage.output_tokens),
        cached_input_tokens: u64::from(response.usage.cached_input_tokens),
        cache_creation_input_tokens: u64::from(response.usage.cache_creation_input_tokens),
        reasoning_tokens: u64::from(response.usage.reasoning_tokens),
    };
    let parsed = parse_typed_response::<T>(&response)?;
    Ok((parsed, usage))
}

/// Plain-text completion — used by `Explainer::explain` and other
/// non-structured surfaces. Mirrors [`structured_completion`]'s
/// system / temperature wiring; returns the assistant text plus the
/// usage tuple.
///
/// **Internal only.** Routes the same `pub(crate)` funnel as
/// [`structured_completion`]; reach this via
/// `DefaultBrain::call_text_traced`.
pub(crate) async fn text_completion(
    chat: &ChatRunnable,
    model: &str,
    system: &str,
    user_prompt: &str,
    max_tokens: u32,
    temperature: Option<f32>,
    ctx: &ExecutionContext,
) -> OxResult<(String, TokenUsage)> {
    let request = ModelRequest {
        model: model.to_owned(),
        messages: vec![Message::user(user_prompt)],
        system: SystemPrompt::cached(system, CacheControl::one_hour()),
        max_tokens: Some(max_tokens),
        temperature,
        ..ModelRequest::default()
    };

    let response = chat
        .complete_request(request, ctx)
        .await
        .map_err(map_entelix_err("LLM request failed"))?;

    metrics::counter!("ox_brain.tokens.input").increment(u64::from(response.usage.input_tokens));
    metrics::counter!("ox_brain.tokens.output").increment(u64::from(response.usage.output_tokens));

    let usage = TokenUsage {
        input_tokens: u64::from(response.usage.input_tokens),
        output_tokens: u64::from(response.usage.output_tokens),
        cached_input_tokens: u64::from(response.usage.cached_input_tokens),
        cache_creation_input_tokens: u64::from(response.usage.cache_creation_input_tokens),
        reasoning_tokens: u64::from(response.usage.reasoning_tokens),
    };
    let text = response.full_text();
    Ok((text, usage))
}

fn parse_typed_response<T>(response: &ModelResponse) -> OxResult<T>
where
    T: serde::de::DeserializeOwned,
{
    if matches!(response.stop_reason, StopReason::MaxTokens) {
        return Err(OxError::Runtime {
            message: format!(
                "LLM output truncated (max_tokens reached); raw length {} chars",
                response.full_text().len()
            ),
        });
    }
    let text = response.full_text();
    let trimmed = text.trim();
    serde_json::from_str(trimmed).map_err(|e| OxError::Runtime {
        message: format!("Failed to parse structured output: {e}\nRaw: {trimmed}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: structured_completion is integration-tested via
    // `crate::test_support::FakeChatRunnable` in the consuming
    // crates (ox-agent, ox-api). The unit tests here pin the
    // pure-function helpers.

    #[test]
    fn parse_typed_response_rejects_max_tokens_truncation() {
        use entelix::ir::{ContentPart, Usage};
        let response = ModelResponse {
            id: "r".into(),
            model: "m".into(),
            stop_reason: StopReason::MaxTokens,
            content: vec![ContentPart::Text {
                text: "{\"partial\":".into(),
                cache_control: None,
                provider_echoes: Vec::new(),
            }],
            usage: Usage::default(),
            rate_limit: None,
            warnings: Vec::new(),
            provider_echoes: Vec::new(),
        };
        let result: OxResult<serde_json::Value> = parse_typed_response(&response);
        assert!(result.is_err());
        let msg = result.err().map(|e| e.to_string()).unwrap_or_default();
        assert!(msg.contains("truncated"));
    }
}
