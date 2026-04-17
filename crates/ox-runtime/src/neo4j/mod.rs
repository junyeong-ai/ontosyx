//! Neo4j graph runtime backend.
//!
//! Module layout:
//! - [`runtime`] — `Neo4jRuntime` struct, connection setup, GraphRuntime impl,
//!   and the thin `Neo4jTransienceDetector` wrapper that delegates to the
//!   shared rules in [`crate::transience`].
//! - [`search`]  — graph exploration (search_nodes, expand_node, graph_overview)
//!
//! Bolt-protocol primitives (parameter binding, retry, batched loads,
//! isolation rewriting) are shared with Memgraph in [`crate::bolt`] —
//! adding a new Bolt backend gets all of those for free.

mod runtime;
mod search;

pub use runtime::{Neo4jRuntime, Neo4jTransienceDetector};
