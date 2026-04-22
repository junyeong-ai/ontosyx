//! Platform-wide background tasks. Each submodule owns one
//! long-running spawn; `main.rs` kicks them off at server boot and
//! shares the workspace-scoped cancellation token so graceful
//! shutdown drains them before the pool closes.

pub mod stale_concepts;

pub use stale_concepts::spawn_stale_concept_scan;
