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
use entelix::tools::ToolRegistry;
use entelix::{ExecutionContext, Result, Runnable};
use sha2::Digest;

use ox_brain::ChatRunnable;

/// Adapter that pairs a [`ChatRunnable`] with the agent's fixed
/// [`SystemPrompt`] and [`ToolSpec`] catalogue.
pub(crate) struct AgentChat {
    inner: ChatRunnable,
    system: SystemPrompt,
    tools: Arc<[ToolSpec]>,
}

impl AgentChat {
    pub(crate) fn new(inner: ChatRunnable, system: SystemPrompt, tools: Vec<ToolSpec>) -> Self {
        Self {
            inner,
            system,
            tools: Arc::from(tools),
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

/// SHA-256 hex of the tool surface — one entry per registered tool,
/// sorted lexically, JSON-serialised. The hash is deterministic
/// across program runs and bumps when a tool's name, description, or
/// input schema changes. The chat handler stamps the result on the
/// `agent_sessions.tool_schema_hash` audit row so replay catches a
/// silent tool-surface drift.
pub(crate) fn compute_tool_schema_hash(registry: &ToolRegistry<()>) -> String {
    let mut entries: Vec<serde_json::Value> = registry
        .names()
        .filter_map(|name| {
            registry.get(name).map(|tool| {
                let metadata = tool.metadata();
                serde_json::json!({
                    "name": metadata.name,
                    "description": metadata.description,
                    "schema": metadata.input_schema,
                })
            })
        })
        .collect();
    entries.sort_by(|a, b| {
        a.get("name")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .cmp(b.get("name").and_then(|v| v.as_str()).unwrap_or_default())
    });
    let serialised = serde_json::to_string(&entries).unwrap_or_default();
    let digest = sha2::Sha256::digest(serialised.as_bytes());
    hex::encode(digest)
}

/// Materialise every registered tool's [`ToolSpec`] in lexical order
/// (by name) — same ordering as [`compute_tool_schema_hash`] so the
/// LLM sees a stable tool catalogue across program runs.
pub(crate) fn collect_tool_specs(registry: &ToolRegistry<()>) -> Vec<ToolSpec> {
    let mut names: Vec<&str> = registry.names().collect();
    names.sort_unstable();
    names
        .into_iter()
        .filter_map(|name| {
            registry
                .get(name)
                .map(|tool| tool.metadata().to_tool_spec())
        })
        .collect()
}
