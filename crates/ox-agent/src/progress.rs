//! Progress channel for tool sub-step events.
//!
//! Re-exports types from `ox_core::progress` and adds the tokio-based
//! channel creation (ox-core is kept lightweight without tokio dependency).

pub use ox_core::progress::{StepStatus, ToolProgress};

pub type ProgressSender = tokio::sync::mpsc::UnboundedSender<ToolProgress>;
pub type ProgressReceiver = tokio::sync::mpsc::UnboundedReceiver<ToolProgress>;

/// Create a progress channel pair.
pub fn channel() -> (ProgressSender, ProgressReceiver) {
    tokio::sync::mpsc::unbounded_channel()
}
