//! [`AgentChat`] — `ChatRunnable` adapter that stamps the agent's
//! persona ([`SystemPrompt`]) and advertised tool surface
//! ([`Arc<[ToolSpec]>`]) onto every dispatch.
//!
//! The agent's tool-loop graph forwards `state.messages` verbatim;
//! the system prompt and tool surface ride on
//! [`entelix::ir::ModelRequest::system`] / `tools` — codec-canonical
//! channels — rather than as extra messages. Without the tools on
//! the request the LLM has no way to know it can call any tools and
//! the ReAct loop never converges; without the system prompt the
//! agent has no persona. Storing the pre-built `SystemPrompt` and
//! `Arc<[ToolSpec]>` keeps the per-call cost two refcount bumps —
//! `SystemPrompt`'s inner `Arc` plus the tool slice.

use std::sync::Arc;

use entelix::ir::{Message, ModelRequest, Role, SystemPrompt, ToolSpec};
use entelix::{ExecutionContext, Result, Runnable};

use ox_brain::ChatRunnable;

/// Adapter that pairs a [`ChatRunnable`] with the agent's fixed
/// [`SystemPrompt`] and [`ToolSpec`] catalogue.
pub(crate) struct AgentChat {
    inner: ChatRunnable,
    system: SystemPrompt,
    tools: Arc<[ToolSpec]>,
}

impl AgentChat {
    pub(crate) fn new(inner: ChatRunnable, system: SystemPrompt, tools: Arc<[ToolSpec]>) -> Self {
        Self {
            inner,
            system,
            tools,
        }
    }
}

#[async_trait::async_trait]
impl Runnable<Vec<Message>, Message> for AgentChat {
    async fn invoke(&self, input: Vec<Message>, ctx: &ExecutionContext) -> Result<Message> {
        let mut request: ModelRequest = self.inner.build_request(input);
        request.system = self.system.clone();
        request.tools = Arc::clone(&self.tools);
        let response = self.inner.complete_request(request, ctx).await?;
        Ok(Message::new(Role::Assistant, response.content))
    }
}
