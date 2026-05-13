//! Re-usable LLM test fixtures.
//!
//! [`FakeChatModel`] is a deterministic, queue-based [`DynChatModel`]
//! impl. Tests enqueue canned [`entelix::ir::ModelResponse`] values
//! ahead of the call; each `complete_request` pops the head of the
//! queue and records the request.
//!
//! Compiles only under `cfg(test)` or with the `test-helpers` cargo
//! feature on. Production binaries never link this module.
//!
//! ## Pattern
//!
//! ```ignore
//! use ox_brain::test_support::{FakeChatModel, make_text_response};
//! use ox_brain::ChatRunnable;
//! use entelix::ExecutionContext;
//!
//! let fake = FakeChatModel::new();
//! fake.enqueue_text(r#"{"answer":42}"#);
//! let chat = fake.into_chat_runnable();
//! let (parsed, _usage): (MyStruct, _) = ox_brain::provider::structured_completion(
//!     &chat, "claude-mock", "system", "user", 256, None, &ExecutionContext::default(),
//! ).await.unwrap();
//! ```

#![cfg(any(test, feature = "test-helpers"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::Mutex;

use async_trait::async_trait;
use entelix::ExecutionContext;
use entelix::ir::{ContentPart, Message, ModelRequest, ModelResponse, StopReason, Usage};

use crate::chat_model::{ChatRunnable, DynChatModel};

/// Deterministic [`DynChatModel`] backed by a FIFO queue of pre-built
/// responses. Reset state between tests by allocating a fresh
/// instance — the type holds no global state.
#[derive(Debug, Default)]
pub struct FakeChatModel {
    queued: Mutex<VecDeque<entelix::Result<ModelResponse>>>,
    requests: Mutex<Vec<ModelRequest>>,
}

impl FakeChatModel {
    /// Create an empty fake. Each test typically enqueues exactly the
    /// responses it expects to consume; an exhausted queue surfaces
    /// as an `entelix::Error` so the test fails loudly rather than
    /// silently producing a default response.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Convenience: queue a successful text response for the next
    /// `complete_request` call. Wraps [`make_text_response`].
    pub fn enqueue_text(&self, text: impl Into<String>) -> &Self {
        self.enqueue_response(make_text_response(text.into()))
    }

    /// Queue a `StopReason::MaxTokens` response — model truncated by
    /// `max_tokens`. Used to pin the truncation-error branch.
    pub fn enqueue_truncated(&self, partial_text: impl Into<String>) -> &Self {
        self.enqueue_response(make_truncated_response(partial_text.into()))
    }

    /// Queue any pre-built response.
    pub fn enqueue_response(&self, response: ModelResponse) -> &Self {
        if let Ok(mut q) = self.queued.lock() {
            q.push_back(Ok(response));
        }
        self
    }

    /// Queue an error for the next `complete_request` call.
    pub fn enqueue_error(&self, error: entelix::Error) -> &Self {
        if let Ok(mut q) = self.queued.lock() {
            q.push_back(Err(error));
        }
        self
    }

    /// Snapshot of every request the fake has seen, in send order.
    /// Useful for asserting prompt content / model / max_tokens.
    pub fn requests(&self) -> Vec<ModelRequest> {
        self.requests
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_default()
    }

    /// `true` when the queue has been fully consumed. Tests that
    /// pre-load N responses can assert exact-N consumption.
    pub fn is_drained(&self) -> bool {
        self.queued.lock().map(|q| q.is_empty()).unwrap_or(true)
    }

    /// Wrap into a [`ChatRunnable`] for direct use against
    /// `structured_completion` / `text_completion` / Brain plumbing.
    #[must_use]
    pub fn into_chat_runnable(self) -> ChatRunnable {
        ChatRunnable::from_arc(Arc::new(self))
    }
}

#[async_trait]
impl DynChatModel for FakeChatModel {
    fn build_request(&self, messages: Vec<Message>) -> ModelRequest {
        ModelRequest {
            model: "fake-model".to_owned(),
            messages,
            ..ModelRequest::default()
        }
    }

    async fn complete_request(
        &self,
        request: ModelRequest,
        _ctx: &ExecutionContext,
    ) -> entelix::Result<ModelResponse> {
        if let Ok(mut requests) = self.requests.lock() {
            requests.push(request);
        }
        let next = self.queued.lock().ok().and_then(|mut q| q.pop_front());
        match next {
            Some(result) => result,
            None => Err(entelix::Error::config(
                "FakeChatModel: queue empty — enqueue a response first",
            )),
        }
    }
}

/// Stop-finish text response with empty usage stats. The id /
/// model strings are stable so request-replay tests can compare
/// the full response shape.
#[must_use]
pub fn make_text_response(text: String) -> ModelResponse {
    ModelResponse {
        id: "fake-response-1".into(),
        model: "fake-model".into(),
        stop_reason: StopReason::EndTurn,
        content: vec![ContentPart::Text {
            text,
            cache_control: None,
            provider_echoes: Vec::new(),
        }],
        usage: Usage::default(),
        rate_limit: None,
        warnings: Vec::new(),
        provider_echoes: Vec::new(),
    }
}

/// Length-truncated response. The text is whatever fragment the
/// model managed to emit before hitting `max_tokens`.
#[must_use]
pub fn make_truncated_response(partial_text: String) -> ModelResponse {
    ModelResponse {
        stop_reason: StopReason::MaxTokens,
        ..make_text_response(partial_text)
    }
}
