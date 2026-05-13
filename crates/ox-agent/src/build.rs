//! Build-time inputs and outputs of [`crate::build_agent`].
//!
//! [`BuildAgentRequest`] is the recipe operators fill at the request
//! boundary; [`BuildAgentResult`] carries the compiled agent plus the
//! audit fingerprint. The six-axis budget recipe lives in
//! [`ox_brain::RunBudgetCaps`] — the workspace's single source of
//! truth shared with the Brain's process-wide default and the
//! chat-wide per-execute scope.

use std::sync::Arc;

use entelix::{Agent, AgentEventSink, ReActState};

use ox_brain::auth::LlmProviderConfig;
use ox_brain::{Brain, ChatModelRegistry};
use ox_context::WorkspaceMode;
use ox_memory::MemoryStore;

use crate::context::DomainContext;
use crate::sinks::RecoveryDetectionConfig;

/// Built-in ceiling used when the caller passes `max_iterations = 0`.
/// Matches the previous hard-coded value so the chat surface keeps
/// the same outer-loop shape.
pub const DEFAULT_RECURSION_LIMIT: u32 = 16;

// ---------------------------------------------------------------------------
// BuildAgentRequest — agent construction parameters
// ---------------------------------------------------------------------------

/// Parameters for constructing an entelix-backed Ontosyx agent.
pub struct BuildAgentRequest {
    /// LLM provider configuration. Resolved into a `ChatRunnable` via
    /// [`BuildAgentRequest::chat_model_registry`] so identical configs
    /// across agent builds share one underlying transport (and
    /// entelix's `reqwest::Client` pool beneath that).
    pub provider_config: LlmProviderConfig,
    /// Process-wide registry that materialises chat-runnable handles
    /// for `provider_config`. The registry caches by
    /// `(provider, credential, region/base_url, model)`, so repeated
    /// agent builds against the same provider re-use the same handle —
    /// without this, every chat session would allocate a fresh
    /// `ChatModel`.
    pub chat_model_registry: Arc<ChatModelRegistry>,
    pub domain: Arc<DomainContext>,
    pub brain: Arc<dyn Brain>,
    pub memory: Option<Arc<MemoryStore>>,
    /// User role for tool access control: "admin", "designer", "viewer".
    pub user_role: String,
    /// Runtime thresholds for the `RecoveryDetectionSink` — minimum
    /// Jaccard similarity for a failure / success pair plus the
    /// per-run window over which the tracker keeps outcomes.
    pub recovery: RecoveryDetectionConfig,
    /// Upper bound on planner iterations (LLM turn + tool batch).
    /// `0` falls back to [`DEFAULT_RECURSION_LIMIT`] so older callers
    /// can omit the field; entelix maps this onto the underlying
    /// state-graph recursion limit.
    pub max_iterations: u32,
    /// Workspace identity propagation — the JWT path produces
    /// [`WorkspaceMode::Workspace`], the API-key / cron path produces
    /// [`WorkspaceMode::SystemBypass`]. Wired into the registry's
    /// `ScopedToolLayer` so every tool dispatch runs under the right
    /// RLS task-locals.
    pub workspace_mode: WorkspaceMode,
    /// Caller-supplied event sinks folded into the agent's fan-out
    /// alongside the domain sinks (`EmbeddingSink`,
    /// `RecoveryDetectionSink`). The HTTP chat handler passes a
    /// per-request [`entelix::ChannelSink`] here so it can forward
    /// every emission to its SSE wire — `Started`, `ToolStart`,
    /// `ToolComplete`, `ToolError`, `Complete`, `Failed` all flow
    /// through one channel.
    pub event_sinks: Vec<Arc<dyn AgentEventSink<ReActState>>>,
    /// Optional `entelix::PolicyLayer` registry — when supplied the
    /// agent's tool registry has the layer stacked so per-tenant PII
    /// redaction (and any future quota / cost extensions) runs around
    /// every tool dispatch. The chat-model side already gets the same
    /// layer through `ChatModelRegistry::with_policy_registry`; the
    /// shared `Arc` keeps both surfaces in lockstep.
    pub policy_registry: Option<Arc<entelix::PolicyRegistry>>,
    /// Reject `RiskLevel::High` queries in `QueryGraphTool` before
    /// they reach the driver.
    pub reject_high_cost: bool,
}

// ---------------------------------------------------------------------------
// BuildAgentResult — what the agent build produces
// ---------------------------------------------------------------------------

/// Result of [`crate::build_agent`].
pub struct BuildAgentResult {
    /// Compiled [`Agent<ReActState>`] ready for `agent.execute(state, &ctx)`.
    pub agent: Agent<ReActState>,
    /// SHA-256 hex of the tool surface the agent was built with —
    /// each registered tool's `(name, description, input_schema)`
    /// triple, sorted by name, JSON-serialised, then hashed. Pinned
    /// at build time so a follow-up tool-surface change forces
    /// re-replay rather than a silent reinterpretation. The chat
    /// handler stamps it on the `agent_sessions.tool_schema_hash`
    /// audit row.
    pub tool_schema_hash: String,
}
