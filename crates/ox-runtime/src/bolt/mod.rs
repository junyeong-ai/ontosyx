//! Shared helpers for backends that speak the Bolt protocol.
//!
//! Both Neo4j and Memgraph use the `neo4rs` driver. Everything that does
//! not depend on backend-specific quirks (parameter binding, JSON value
//! coercion, exponential-backoff retry, identifier validation, workspace
//! isolation injection from task-locals, the `FuturesUnordered`-based
//! batch loader) lives here so a new Bolt backend gets it for free.

pub(crate) mod helpers;
pub(crate) mod isolation;
pub(crate) mod load;
pub(crate) mod retry;

pub(crate) use helpers::{
    bind_params, json_to_property_value, truncate_query, validate_identifier,
};
pub(crate) use isolation::scope_with_task_locals;
pub(crate) use load::{LoadContext, run_batched_load};
pub(crate) use retry::{RetryConfig, with_retry};
