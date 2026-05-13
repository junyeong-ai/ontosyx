//! `ChatRunnable` — provider-erased handle to an entelix `ChatModel<C, T>`.
//!
//! The Brain layer routes through one type for all vendor variants:
//! - It satisfies [`entelix::Runnable<Vec<Message>, Message>`] so a
//!   `ReActAgentBuilder` accepts it directly.
//! - It exposes a fully-formed [`ModelRequest`] dispatch path so
//!   `structured_completion` can vary the system prompt and attach a
//!   `ResponseFormat` per call without re-building the model.
//!
//! The vendor variation lives behind one `Arc<dyn DynChatModel>`.
//! `ChatModel<C, T>` for every `(Codec, Transport)` pair already
//! implements [`entelix::Runnable`] in `entelix-runnable/src/chat.rs`,
//! so the implementation here is mechanical: forward the request
//! shape onto the inner model's service stack.
//!
//! ## Cross-cutting via `entelix-policy`
//!
//! Cost observation, pre-call gates, PII redaction, and quota
//! enforcement ride [`entelix::PolicyLayer`] applied at
//! [`ChatModel`] construction in [`crate::chat_model_factory`]. The
//! layer pre-checks `RunBudget`'s token + cost axes, charges the
//! tenant ledger on the `Ok` branch, and surfaces typed
//! `UsageLimitExceeded` breaches before the wire roundtrip fires —
//! no per-consumer decorator wiring.

use std::sync::Arc;

use async_trait::async_trait;
use entelix::ir::{Message, ModelRequest, ModelResponse, Role};
use entelix::service::ModelInvocation;
use entelix::{ExecutionContext, Result, Runnable, codecs::Codec, transports::Transport};
use tower::ServiceExt;

/// Object-safe view of an entelix `ChatModel<C, T>`.
///
/// Two methods, both essential:
///
/// - [`Self::build_request`] returns a [`ModelRequest`] pre-populated
///   with the model's configured shape (system prompt, max_tokens,
///   tools, …). Brain operations that want the configured shape but
///   override one field (e.g. attach a [`crate::provider::ResponseFormat`])
///   call this then mutate.
/// - [`Self::complete_request`] dispatches a fully-formed request
///   through the model's codec + transport pipeline, mirroring
///   `entelix::ChatModel::complete_full`'s `RunBudget` pre-/post-call
///   accounting.
#[async_trait]
pub trait DynChatModel: Send + Sync + 'static {
    /// Build a request from the model's configured shape and the
    /// supplied conversation. Equivalent to
    /// `entelix::ChatModelConfig::build_request`, surfaced here so
    /// callers can reach the request shape across an `Arc<dyn _>`.
    fn build_request(&self, messages: Vec<Message>) -> ModelRequest;

    /// Dispatch a fully-formed [`ModelRequest`] through the model's
    /// codec + transport pipeline.
    ///
    /// Mirrors `entelix::ChatModel::complete_full`'s `RunBudget`
    /// pre-/post-call accounting so structured-output dispatches
    /// honour the same six-axis cap as plain completions.
    async fn complete_request(
        &self,
        request: ModelRequest,
        ctx: &ExecutionContext,
    ) -> Result<ModelResponse>;
}

#[async_trait]
impl<C, T> DynChatModel for entelix::ChatModel<C, T>
where
    C: Codec,
    T: Transport,
{
    fn build_request(&self, messages: Vec<Message>) -> ModelRequest {
        self.config().build_request(messages)
    }

    async fn complete_request(
        &self,
        request: ModelRequest,
        ctx: &ExecutionContext,
    ) -> Result<ModelResponse> {
        let invocation = ModelInvocation::new(request, ctx.clone());
        let budget = ctx.run_budget();
        if let Some(b) = &budget {
            b.check_pre_request()?;
        }
        let response = self.service().oneshot(invocation).await?;
        if let Some(b) = &budget {
            b.observe_usage(&response.usage)?;
        }
        Ok(response)
    }
}

/// Cheap-to-clone, vendor-erased chat-completion handle.
///
/// `ChatRunnable` is the canonical return type of
/// [`crate::chat_model_factory::build_chat_model`] and the canonical
/// argument to `ReActAgentBuilder::new` in ox-agent. It satisfies
/// [`entelix::Runnable<Vec<Message>, Message>`] (so the agent builder
/// accepts it directly) and forwards the structured-completion
/// dispatch path to the underlying [`DynChatModel`].
#[derive(Clone)]
pub struct ChatRunnable {
    inner: Arc<dyn DynChatModel>,
}

impl ChatRunnable {
    /// Wrap any [`DynChatModel`] (typically a freshly-built
    /// [`entelix::ChatModel<C, T>`]) into the vendor-erased handle.
    #[must_use]
    pub fn new<M>(model: M) -> Self
    where
        M: DynChatModel,
    {
        Self {
            inner: Arc::new(model),
        }
    }

    /// Wrap an already-`Arc`-shared [`DynChatModel`]. Used by the
    /// registry to share one cached model across many lookups.
    #[must_use]
    pub fn from_arc(inner: Arc<dyn DynChatModel>) -> Self {
        Self { inner }
    }

    /// Build a request from the model's configured shape — used by
    /// `structured_completion` to start from the configured baseline
    /// (validation retries, max_tokens) before mutating per-call
    /// fields (`system`, `response_format`).
    #[must_use]
    pub fn build_request(&self, messages: Vec<Message>) -> ModelRequest {
        self.inner.build_request(messages)
    }

    /// Dispatch a fully-formed request through the model.
    pub async fn complete_request(
        &self,
        request: ModelRequest,
        ctx: &ExecutionContext,
    ) -> Result<ModelResponse> {
        self.inner.complete_request(request, ctx).await
    }

    /// Convenience: dispatch with the model's configured request
    /// shape and the supplied conversation. Mirrors
    /// `entelix::ChatModel::complete_full`.
    pub async fn complete_full(
        &self,
        messages: Vec<Message>,
        ctx: &ExecutionContext,
    ) -> Result<ModelResponse> {
        let request = self.inner.build_request(messages);
        self.inner.complete_request(request, ctx).await
    }
}

impl std::fmt::Debug for ChatRunnable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChatRunnable")
            .field("inner", &"<dyn DynChatModel>")
            .finish()
    }
}

#[async_trait]
impl Runnable<Vec<Message>, Message> for ChatRunnable {
    async fn invoke(&self, input: Vec<Message>, ctx: &ExecutionContext) -> Result<Message> {
        let response = self.complete_full(input, ctx).await?;
        Ok(Message::new(Role::Assistant, response.content))
    }
}
