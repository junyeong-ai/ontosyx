//! `BrainChatModel` — object-safe erasure of `entelix::ChatModel<C, T>`.
//!
//! `entelix::ChatModel<C: Codec, T: Transport>` is a generic struct
//! parameterised on codec + transport, so `Arc<dyn ChatModel>` is
//! impossible. The [`crate::client_pool::ClientPool`] routes requests by
//! runtime provider string (`"anthropic"`, `"bedrock"`, `"claude-code"`,
//! …), which forces the erasure on this side of the integration —
//! entelix internals deliberately keep `Codec` and `Transport`
//! monomorphic for hot-path inlining.
//!
//! Two methods are exposed: [`BrainChatModel::complete_full`] mirrors
//! `ChatModel::complete_full` (one-shot) and
//! [`BrainChatModel::stream_deltas`] mirrors `ChatModel::stream_deltas`
//! (streaming). Both delegate verbatim — this trait adds no policy,
//! retry, or budgeting on top of entelix; it exists purely to allow
//! `Arc<dyn BrainChatModel>` storage in the pool.

use std::sync::Arc;

use async_trait::async_trait;
use entelix::codecs::Codec;
use entelix::ir::{Message, ModelResponse};
use entelix::service::ModelStream;
use entelix::transports::Transport;
use entelix::{ChatModel, ExecutionContext, Result};

/// Object-safe facade over `entelix::ChatModel<C, T>`.
#[async_trait]
pub trait BrainChatModel: Send + Sync + 'static {
    /// Provider identifier matching `LlmProviderConfig::provider`
    /// (`"anthropic"`, `"bedrock"`, `"claude-code"`, …). Used by the
    /// pool's `by_provider` lookup and for telemetry tags.
    fn provider(&self) -> &str;

    /// Model id as understood by the underlying provider
    /// (`"claude-opus-4-7"`,
    /// `"anthropic.claude-3-5-sonnet-20241022-v2:0"`, …).
    fn model(&self) -> &str;

    /// One-shot completion. Delegates to
    /// [`entelix::ChatModel::complete_full`].
    async fn complete_full(
        &self,
        messages: Vec<Message>,
        ctx: &ExecutionContext,
    ) -> Result<ModelResponse>;

    /// Streaming completion. Delegates to
    /// [`entelix::ChatModel::stream_deltas`].
    async fn stream_deltas(
        &self,
        messages: Vec<Message>,
        ctx: &ExecutionContext,
    ) -> Result<ModelStream>;
}

/// Generic concrete impl wrapping a built `ChatModel<C, T>` with its
/// provider + model identity tags.
pub struct BrainChatModelImpl<C: Codec + 'static, T: Transport + 'static> {
    inner: ChatModel<C, T>,
    provider: String,
    model: String,
}

impl<C: Codec + 'static, T: Transport + 'static> BrainChatModelImpl<C, T> {
    pub fn new(
        inner: ChatModel<C, T>,
        provider: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        Self {
            inner,
            provider: provider.into(),
            model: model.into(),
        }
    }

    /// Borrow the underlying typed model — useful when downstream
    /// code needs codec/transport-specific surface (rare; the pool
    /// path is `Arc<dyn BrainChatModel>` everywhere else).
    pub const fn inner(&self) -> &ChatModel<C, T> {
        &self.inner
    }
}

#[async_trait]
impl<C: Codec + 'static, T: Transport + 'static> BrainChatModel for BrainChatModelImpl<C, T> {
    fn provider(&self) -> &str {
        &self.provider
    }

    fn model(&self) -> &str {
        &self.model
    }

    async fn complete_full(
        &self,
        messages: Vec<Message>,
        ctx: &ExecutionContext,
    ) -> Result<ModelResponse> {
        self.inner.complete_full(messages, ctx).await
    }

    async fn stream_deltas(
        &self,
        messages: Vec<Message>,
        ctx: &ExecutionContext,
    ) -> Result<ModelStream> {
        self.inner.stream_deltas(messages, ctx).await
    }
}

/// Type alias for the pool's stored value — `Arc<dyn BrainChatModel>`.
pub type SharedChatModel = Arc<dyn BrainChatModel>;
