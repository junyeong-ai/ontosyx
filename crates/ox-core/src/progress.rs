//! Sub-step progress events for long-running operations.
//!
//! Type definitions shared between `ox-brain` (translate_query sub-steps)
//! and `ox-agent` (tool-level steps). Channel creation is in `ox-agent`
//! since it depends on tokio (ox-core is kept lightweight).

use serde::Serialize;

/// Sub-step progress event emitted during long-running operations.
///
/// Events are correlated to the running tool call by `tool_name` in
/// the SSE handler, which maps it to branchforge's tool call ID.
#[derive(Debug, Clone, Serialize)]
pub struct ToolProgress {
    /// Tool name for correlation (e.g., `"query_graph"`).
    pub tool_name: String,
    /// Machine-readable step identifier (e.g., `"schema_discovery"`, `"llm_primary"`).
    pub step: String,
    /// Step lifecycle status.
    pub status: StepStatus,
    /// Step duration in milliseconds (set on Completed/Failed, None on Started).
    pub duration_ms: Option<u64>,
    /// Step-specific metadata.
    pub metadata: Option<serde_json::Value>,
}

/// Status of a progress step.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    Started,
    Completed,
    Failed,
}
