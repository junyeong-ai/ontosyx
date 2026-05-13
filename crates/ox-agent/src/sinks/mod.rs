//! Agent-side sinks — observe-only consumers of
//! [`entelix::AgentEvent<ReActState>`].
//!
//! Two domain sinks ship: [`EmbeddingSink`] pushes tool-output snippets
//! into the long-term memory store; [`RecoveryDetectionSink`] watches
//! for failure → success patterns on `query_graph` calls and persists
//! a verified `correction` row in the knowledge bank so future RAG
//! injects the lesson. Composition into one fan-out is handled by
//! [`entelix::FanOutSink`] at the agent-build site.
//!
//! Both sinks return `Ok(())` for every event variant — `Err` from
//! a sink halts the agent runtime, and these workloads are
//! observability + side-effect, never load-bearing on the agent's path.
//! Internal failures log via `tracing::warn!` and drop.

mod embedding;
mod recovery_detection;

pub use embedding::EmbeddingSink;
pub use recovery_detection::{RecoveryDetectionConfig, RecoveryDetectionSink};
