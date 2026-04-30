//! Platform-wide background tasks. Each submodule owns one
//! long-running spawn; `main.rs` kicks them off at server boot and
//! shares the workspace-scoped cancellation token so graceful
//! shutdown drains them before the pool closes.
//!
//! Every cron implements [`CronTask`] and routes through the
//! shared [`spawn_cron`] scheduler — the pattern that used to be
//! hand-rolled per module (ticker + select! + tracing) now lives
//! in one place, and a new cron takes ~20 lines total.

pub mod clarification_evict;
pub mod cron;
pub mod draft_checkpoint_cleanup;
pub mod quality_baseline;
pub mod soft_delete_compaction;
pub mod stale_concepts;

pub use clarification_evict::spawn_clarification_evict;
pub use cron::{CronTask, spawn_cron};
pub use draft_checkpoint_cleanup::spawn_draft_checkpoint_cleanup;
pub use quality_baseline::spawn_quality_baseline_scan;
pub use soft_delete_compaction::spawn_soft_delete_compaction;
pub use stale_concepts::spawn_stale_concept_scan;
