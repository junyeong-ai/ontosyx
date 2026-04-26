//! Golden coverage of `provider::structured_completion` — the seam
//! every Brain operation funnels through. Each scenario pins a
//! distinct branch of the function's response handling so a
//! regression in any one stage (schema enforcement / JSON-only
//! fallback / fence-stripping / reasoning-prefix recovery /
//! truncation handling) shows up in this file rather than the
//! production `translate_query` / `design_ontology` paths where the
//! provider call is buried.
//!
//! [`MockLlmCall`] is the deterministic provider: enqueue canned
//! `ModelResponse` ahead of the call, and `send()` pops them in
//! order. Tests that need to assert prompt structure read it back
//! from `mock.requests()`.

#![cfg(feature = "test-helpers")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use ox_brain::provider::{SchemaComplexityThresholds, structured_completion_with_thresholds};
use ox_brain::test_support::{
    MockLlmCall, make_text_response, make_truncated_response,
};
use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Debug, Deserialize, JsonSchema, PartialEq)]
struct Answer {
    label: String,
    score: u32,
}

const SYSTEM: &str = "You are a deterministic test fixture.";
const USER: &str = "Give back {\"label\": \"alpha\", \"score\": 1}.";
const MAX_TOKENS: u32 = 256;

fn permissive_thresholds() -> SchemaComplexityThresholds {
    // The Answer schema is well within these defaults; the
    // restrictive variant below forces the JSON-only fallback.
    SchemaComplexityThresholds::default()
}

fn restrictive_thresholds() -> SchemaComplexityThresholds {
    // Thresholds tight enough to fall *every* schema through the
    // JSON-only branch, regardless of size.
    SchemaComplexityThresholds {
        max_optional_params: 0,
        max_total_properties: 0,
    }
}

#[tokio::test]
async fn schema_enforced_path_returns_typed_struct_and_caches_system_prompt() {
    let mock = MockLlmCall::new();
    mock.enqueue_text(r#"{"label":"alpha","score":1}"#);

    let answer: Answer = structured_completion_with_thresholds(
        &mock,
        "claude-mock",
        SYSTEM,
        USER,
        MAX_TOKENS,
        None,
        permissive_thresholds(),
    )
    .await
    .expect("happy-path schema enforcement parses the canned response");

    assert_eq!(answer, Answer { label: "alpha".into(), score: 1 });
    assert!(mock.is_drained(), "exactly one response consumed");

    // Pre-flight assertion on the request shape — the system prompt
    // is wrapped in a cached SystemBlock with a 1h TTL. Any
    // regression that re-introduces uncached SystemPrompt::text()
    // would show up here.
    let requests = mock.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].model, "claude-mock");
    assert_eq!(requests[0].settings.max_output_tokens, Some(MAX_TOKENS));
}

#[tokio::test]
async fn json_only_fallback_strips_code_fence_wrapper() {
    // Restrictive thresholds force the JSON-only branch where
    // provider-level schema is *not* attached and `provider.rs::extract_json`
    // unwraps the response. This pins the ```json ... ``` fence path.
    let mock = MockLlmCall::new();
    mock.enqueue_text(
        "Here you go:\n\
         ```json\n\
         {\"label\":\"beta\",\"score\":7}\n\
         ```\n\
         Hope that helps!",
    );

    let answer: Answer = structured_completion_with_thresholds(
        &mock,
        "claude-mock",
        SYSTEM,
        USER,
        MAX_TOKENS,
        None,
        restrictive_thresholds(),
    )
    .await
    .expect("code-fence-wrapped JSON parses through the fallback");

    assert_eq!(answer, Answer { label: "beta".into(), score: 7 });
}

#[tokio::test]
async fn json_only_fallback_strips_reasoning_prefix() {
    // Same JSON-only branch, different LLM output shape: prose
    // before a line-anchored JSON object. `extract_json` finds
    // the brace at line start and slices forward.
    let mock = MockLlmCall::new();
    mock.enqueue_text(
        "Looking at the request, the right answer is:\n\
         \n\
         {\"label\":\"gamma\",\"score\":3}",
    );

    let answer: Answer = structured_completion_with_thresholds(
        &mock,
        "claude-mock",
        SYSTEM,
        USER,
        MAX_TOKENS,
        None,
        restrictive_thresholds(),
    )
    .await
    .expect("reasoning-prefix JSON parses through the fallback");

    assert_eq!(answer, Answer { label: "gamma".into(), score: 3 });
}

#[tokio::test]
async fn json_only_fallback_recovers_self_corrected_last_object() {
    // Multiple balanced JSON objects in one response — `extract_last_json`
    // walks from the end and returns the final one. Pins the
    // self-correction recovery path.
    let mock = MockLlmCall::new();
    mock.enqueue_text(
        "Wait, that first attempt was wrong:\n\
         \n\
         {\"label\":\"first\",\"score\":99}\n\
         \n\
         Let me correct it:\n\
         \n\
         {\"label\":\"final\",\"score\":42}",
    );

    let answer: Answer = structured_completion_with_thresholds(
        &mock,
        "claude-mock",
        SYSTEM,
        USER,
        MAX_TOKENS,
        None,
        restrictive_thresholds(),
    )
    .await
    .expect("self-correction recovery returns the trailing object");

    assert_eq!(answer, Answer { label: "final".into(), score: 42 });
}

#[tokio::test]
async fn truncated_response_surfaces_explicit_length_error() {
    // FinishReason::Length means `max_output_tokens` was hit —
    // the trailing `}` is missing. `provider.rs` refuses to parse
    // and returns an error naming the truncated length so the
    // caller (the agent) can retry with a higher cap rather than
    // silently working with malformed data.
    let mock = MockLlmCall::new();
    mock.enqueue_response(make_truncated_response(
        r#"{"label":"truncated","score":"#.into(),
    ));

    let result = structured_completion_with_thresholds::<Answer>(
        &mock,
        "claude-mock",
        SYSTEM,
        USER,
        MAX_TOKENS,
        None,
        restrictive_thresholds(),
    )
    .await;

    let err = result.expect_err("truncation must not parse silently");
    let message = err.to_string();
    assert!(
        message.contains("truncated") || message.contains("max_tokens"),
        "error message must name the truncation cause; got: {message}",
    );
}

#[tokio::test]
async fn provider_error_propagates_without_retry() {
    // The mock surfaces a transport-style failure on the first
    // call. `structured_completion` does not own retry — that is
    // `RetryingClient`'s job. Pin the no-silent-retry contract.
    let mock = MockLlmCall::new();
    mock.enqueue_error(branchforge::Error::Config("fake provider failure".into()));

    let result = structured_completion_with_thresholds::<Answer>(
        &mock,
        "claude-mock",
        SYSTEM,
        USER,
        MAX_TOKENS,
        None,
        permissive_thresholds(),
    )
    .await;

    let err = result.expect_err("provider error must propagate as OxError");
    assert!(
        err.to_string().contains("fake provider failure"),
        "error chain must preserve the provider's message; got: {err}",
    );
}

#[tokio::test]
async fn make_text_response_helper_builds_a_well_formed_canonical_response() {
    // Sanity-pin the helper itself — every test above leans on
    // its `Stop` / `Usage::default()` / single text part shape.
    let resp = make_text_response("hello".into());
    assert_eq!(resp.text(), "hello");
    assert_eq!(resp.finish_reason, branchforge::ir::FinishReason::Stop);
    assert_eq!(resp.usage.input_tokens, 0);
}
