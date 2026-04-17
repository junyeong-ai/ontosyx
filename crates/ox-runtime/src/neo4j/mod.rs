//! Neo4j graph runtime backend.
//!
//! Module layout:
//! - [`runtime`]    — `Neo4jRuntime` struct, connection setup, GraphRuntime impl
//! - [`search`]     — graph exploration (search_nodes, expand_node, graph_overview)
//! - [`transience`] — Neo4j-specific transient error detection
//!
//! Bolt-protocol primitives (parameter binding, retry, batched loads,
//! isolation rewriting) are shared with Memgraph in [`crate::bolt`] —
//! adding a new Bolt backend gets all of those for free.

mod runtime;
mod search;
mod transience;

pub use runtime::Neo4jRuntime;
pub use transience::Neo4jTransienceDetector;
