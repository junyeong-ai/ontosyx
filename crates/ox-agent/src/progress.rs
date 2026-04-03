//! Sub-step progress events for long-running tools.
//!
//! Tools like `query_graph` have multiple internal steps (translate, compile, execute).
//! This module provides a typed progress channel that tools emit to, and the SSE
//! handler in `ox-api` merges into the client event stream as `tool_progress` events.
//!
//! # Architecture
//!
//! ```text
//! QueryGraphTool          chat.rs SSE Handler          Frontend
//!      │                       │                          │
//!      ├─ tx.send(progress) →  ├─ progress_rx.recv()      │
//!      │                       ├─ correlate to tool_call   │
//!      │                       ├─ SSE tool_progress  ──→   ├─ update ToolCall.steps
//! ```

use serde::Serialize;

/// Sub-step progress event emitted by long-running tools.
///
/// The SSE handler correlates these events to the currently running tool call
/// by matching `tool_name` against the `running_tools` map populated from
/// branchforge `ToolStart` events.
#[derive(Debug, Clone, Serialize)]
pub struct ToolProgress {
    /// Tool name for correlation (e.g., `"query_graph"`).
    pub tool_name: String,
    /// Machine-readable step identifier (e.g., `"translating"`, `"compiling"`).
    pub step: String,
    /// Step lifecycle status.
    pub status: StepStatus,
    /// 0-based step index within this tool execution.
    pub step_index: u32,
    /// Total number of steps expected.
    pub total_steps: u32,
    /// Step duration in milliseconds (set on Completed/Failed, None on Started).
    pub duration_ms: Option<u64>,
    /// Step-specific metadata. Schema per step:
    /// - `"compiling"` completed: `{"cypher": "<compiled query>"}`
    /// - `"executing"` completed: `{"row_count": <number>}`
    /// - All others: `null`
    pub metadata: Option<serde_json::Value>,
}

/// Status of a tool execution step.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    Started,
    Completed,
    Failed,
}

pub type ProgressSender = tokio::sync::mpsc::UnboundedSender<ToolProgress>;
pub type ProgressReceiver = tokio::sync::mpsc::UnboundedReceiver<ToolProgress>;

/// Create a progress channel pair.
pub fn channel() -> (ProgressSender, ProgressReceiver) {
    tokio::sync::mpsc::unbounded_channel()
}
