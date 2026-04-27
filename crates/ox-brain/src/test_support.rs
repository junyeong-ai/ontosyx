//! Re-usable LLM test fixtures.
//!
//! [`MockLlmCall`] is a deterministic, queue-based [`branchforge::client::LlmCall`]
//! impl. Tests enqueue canned [`branchforge::ir::ModelResponse`] values
//! ahead of the call; each `send()` pops the head of the queue and
//! records the request. `send_stream()` is intentionally unimplemented —
//! callers that need streaming should use a streaming-aware fixture
//! when one is added.
//!
//! Compiles only under `cfg(test)` or with the `test-helpers` cargo
//! feature on. Production binaries never link this module.
//!
//! ## Pattern
//!
//! ```ignore
//! use ox_brain::test_support::{MockLlmCall, make_text_response};
//!
//! let mock = MockLlmCall::new();
//! mock.enqueue_text(r#"{"answer":42}"#);
//! let result: MyStruct = ox_brain::provider::structured_completion(
//!     &mock, "claude-mock", "system", "user", 256, None,
//! ).await.unwrap();
//! ```

#![cfg(any(test, feature = "test-helpers"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::VecDeque;
use std::sync::Mutex;

use async_trait::async_trait;
use branchforge::client::LlmCall;
use branchforge::client::provider_client::ChunkStream;
use branchforge::ir::stream::ModelStreamChunk;
use branchforge::ir::{ContentPart, FinishReason, ModelRequest, ModelResponse, Role, Usage};
use tokio_util::sync::CancellationToken;

/// Deterministic [`LlmCall`] backed by a FIFO queue of pre-built
/// responses. Reset state between tests by allocating a fresh
/// instance — the type holds no global state.
#[derive(Debug, Default)]
pub struct MockLlmCall {
    queued: Mutex<VecDeque<branchforge::Result<ModelResponse>>>,
    /// Pre-built streaming responses, one full chunk vec per
    /// `send_stream` call. Each call pops the head of the queue and
    /// returns its chunks. Empty queue ⇒ Config error so the test
    /// fails loudly rather than hanging on a phantom stream.
    streams: Mutex<VecDeque<Vec<ModelStreamChunk>>>,
    requests: Mutex<Vec<ModelRequest>>,
}

impl MockLlmCall {
    /// Create an empty mock. Each test typically enqueues exactly the
    /// responses it expects to consume; an exhausted queue surfaces
    /// as a `branchforge::Error::Config` so the test fails loudly.
    pub fn new() -> Self {
        Self::default()
    }

    /// Queue a successful text response for the next `send()` call.
    /// Convenience wrapper around [`Self::enqueue_response`].
    pub fn enqueue_text(&self, text: impl Into<String>) -> &Self {
        self.enqueue_response(make_text_response(text.into()))
    }

    /// Queue a `FinishReason::Length` response — model truncated by
    /// `max_output_tokens`. Used to pin the truncation-error branch.
    pub fn enqueue_truncated(&self, partial_text: impl Into<String>) -> &Self {
        self.enqueue_response(make_truncated_response(partial_text.into()))
    }

    /// Queue any pre-built response.
    pub fn enqueue_response(&self, response: ModelResponse) -> &Self {
        self.queued.lock().unwrap().push_back(Ok(response));
        self
    }

    /// Queue an error for the next `send()` call.
    pub fn enqueue_error(&self, error: branchforge::Error) -> &Self {
        self.queued.lock().unwrap().push_back(Err(error));
        self
    }

    /// Snapshot of every request the mock has seen, in send order.
    /// Useful for asserting prompt content / model / max_tokens.
    pub fn requests(&self) -> Vec<ModelRequest> {
        self.requests.lock().unwrap().clone()
    }

    /// `true` when the queue has been fully consumed. Tests that
    /// pre-load N responses can assert exact-N consumption.
    pub fn is_drained(&self) -> bool {
        self.queued.lock().unwrap().is_empty()
    }

    /// Queue a streaming response — one `Vec<ModelStreamChunk>` is
    /// emitted by the next `send_stream` call. Each chunk yields
    /// `Ok(chunk)` to the consumer; tests that need an
    /// `Err`-bearing chunk assemble the vec via
    /// [`make_chunked_stream_with_errors`] (or hand-roll one).
    pub fn enqueue_stream(&self, chunks: Vec<ModelStreamChunk>) -> &Self {
        self.streams.lock().unwrap().push_back(chunks);
        self
    }
}

#[async_trait]
impl LlmCall for MockLlmCall {
    async fn send(&self, request: &ModelRequest) -> branchforge::Result<ModelResponse> {
        self.requests.lock().unwrap().push(request.clone());
        self.queued
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| {
                Err(branchforge::Error::Config(
                    "MockLlmCall queue empty — enqueue a response before calling send()".into(),
                ))
            })
    }

    async fn send_stream(
        &self,
        request: &ModelRequest,
        cancel_token: CancellationToken,
    ) -> branchforge::Result<ChunkStream> {
        self.requests.lock().unwrap().push(request.clone());

        let chunks = self
            .streams
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| {
                branchforge::Error::Config(
                    "MockLlmCall stream queue empty — enqueue_stream(...) before \
                     calling send_stream()"
                        .into(),
                )
            })?;

        // Yield `Ok(chunk)` for every chunk in the pre-built vec, but
        // honour cancellation between chunks so cancellation-aware
        // tests can trigger a mid-stream stop.
        let stream = async_stream::stream! {
            for chunk in chunks {
                if cancel_token.is_cancelled() {
                    yield Err(branchforge::Error::Config(
                        "stream cancelled mid-flight".into(),
                    ));
                    break;
                }
                yield Ok(chunk);
            }
        };
        Ok(Box::pin(stream))
    }
}

/// Stop-finish text response with empty usage stats. The id /
/// model strings are stable so request-replay tests can compare
/// the full response shape.
pub fn make_text_response(text: String) -> ModelResponse {
    ModelResponse {
        id: "mock-response-1".into(),
        model: "mock-model".into(),
        content: vec![ContentPart::text(text)],
        finish_reason: FinishReason::Stop,
        usage: Usage::default(),
        continuation: None,
        warnings: vec![],
        raw: None,
        rate_limit: None,
    }
}

/// Length-truncated response. The text is whatever fragment the
/// model managed to emit before hitting `max_output_tokens`.
pub fn make_truncated_response(partial_text: String) -> ModelResponse {
    ModelResponse {
        finish_reason: FinishReason::Length,
        ..make_text_response(partial_text)
    }
}

/// Streaming response built from a single concatenated text. Emits
/// `MessageStart` → one `TextDelta` carrying the full text →
/// `Finish(Stop)`. Use [`make_chunked_stream`] when the test cares
/// about delta boundaries.
pub fn make_text_stream(text: impl Into<String>) -> Vec<ModelStreamChunk> {
    vec![
        ModelStreamChunk::MessageStart {
            id: "mock-response-1".into(),
            model: "mock-model".into(),
            role: Role::Assistant,
        },
        ModelStreamChunk::TextDelta {
            index: 0,
            text: text.into(),
        },
        ModelStreamChunk::Finish {
            reason: FinishReason::Stop,
            usage: Usage::default(),
        },
    ]
}

/// Streaming response with explicit text-delta boundaries — each
/// `&str` becomes one `TextDelta` chunk. Tests verify chunk
/// reassembly by passing multi-segment input and asserting the
/// concatenated text on the consumer side.
pub fn make_chunked_stream<'a>(
    chunks: impl IntoIterator<Item = &'a str>,
) -> Vec<ModelStreamChunk> {
    let mut out = vec![ModelStreamChunk::MessageStart {
        id: "mock-response-1".into(),
        model: "mock-model".into(),
        role: Role::Assistant,
    }];
    for piece in chunks {
        out.push(ModelStreamChunk::TextDelta {
            index: 0,
            text: piece.to_string(),
        });
    }
    out.push(ModelStreamChunk::Finish {
        reason: FinishReason::Stop,
        usage: Usage::default(),
    });
    out
}
