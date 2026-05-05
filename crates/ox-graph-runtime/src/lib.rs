#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unreachable
    )
)]

mod bolt;
pub mod action_executor;
pub mod dialect;
pub mod enrichment;
pub mod isolation;
pub mod memgraph;
pub mod neo4j;
pub mod profiler;
pub mod registry;
pub mod transience;

// Convenience re-export so external callers (ox-api / ox-agent /
// ox-brain) keep referring to `cypher::*`; the dialect-aware module
// layout stays internal. Future graph dialects (GQL, Gremlin) sit
// beside cypher under `dialect::*` without touching this re-export.
pub use dialect::cypher;

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;

use ox_core::error::{OxError, OxResult};
use ox_ontology::ir::OntologyIR;

// ---------------------------------------------------------------------------
// Per-request graph workspace context via task-local
// ---------------------------------------------------------------------------
// Set by the workspace middleware. Read by Neo4jRuntime to scope graph queries.
// Mirrors ox_store::WORKSPACE_ID / SYSTEM_BYPASS for the graph layer.
//
// GRAPH_ONTOLOGY carries the active OntologyIR snapshot so the Cypher
// validator pipeline can flag unknown labels / relationships / properties
// before a query hits the driver. Internal paths that have no authored
// query to validate (search, profiler, schema introspection) simply do
// not set the task-local — the ontology validator is skipped and only
// safety + workspace-scope passes run.
// ---------------------------------------------------------------------------

tokio::task_local! {
    /// Per-request workspace ID for graph isolation.
    pub static GRAPH_WORKSPACE_ID: Uuid;
    /// When true, graph queries bypass workspace isolation (system tasks).
    pub static GRAPH_SYSTEM_BYPASS: bool;
    /// Active ontology snapshot for the OntologyValidator pre-execute gate.
    pub static GRAPH_ONTOLOGY: Arc<OntologyIR>;
    /// Optional bitemporal anchor — when set, the bolt pipeline
    /// projects the active ontology through `OntologyIR::as_of(at)`
    /// before validation. Rules / mappings / property bindings whose
    /// effective window doesn't cover `at` are filtered out for the
    /// duration of the request, so a "what would this query have
    /// returned at 2024-01-01?" call resolves against the historical
    /// shape rather than today's. Unset → today's IR is used as-is.
    pub static GRAPH_ONTOLOGY_AS_OF: chrono::DateTime<chrono::Utc>;
    /// Per-request statement timeout. Backends that honour this
    /// (Neo4j, Memgraph) wrap `execute_query_raw` / `execute_load_raw`
    /// in `tokio::time::timeout(_, …)` so a runaway query — accidental
    /// Cartesian, unbounded var-length, missing index — surfaces as
    /// `OxError::Runtime("query timed out after Xs")` rather than
    /// holding a connection forever. Unset → backend default
    /// ([`DEFAULT_QUERY_TIMEOUT`]).
    pub static GRAPH_QUERY_TIMEOUT: std::time::Duration;
    /// Authenticated principal driving the current request. Drives the
    /// `AclRewriter` Deny / Mask passes plus future ABAC predicates.
    /// Absence skips the ACL pass entirely — system-task callers
    /// (`spawn_system`, retention compaction) leave it unset and run
    /// under [`GRAPH_SYSTEM_BYPASS`] instead.
    pub static GRAPH_PRINCIPAL: cypher::RequestPrincipal;
    /// Pre-loaded ACL policy snapshot for the current principal in
    /// the current workspace, sorted priority-desc. Loaded once per
    /// request by the API layer and threaded through every Cypher
    /// pass via `RewriteContext`.
    pub static GRAPH_ACL_SNAPSHOT: Arc<cypher::AclSnapshot>;
}

/// Backend-default query timeout when the caller has not bound
/// [`GRAPH_QUERY_TIMEOUT`]. Long enough to clear most heuristic
/// reports, short enough that a Cartesian-product slip-up doesn't
/// burn the connection pool. Override via the task-local for
/// long-running analytics — `tokio::time::timeout`'s overhead is
/// constant and trivial relative to a graph traversal.
pub const DEFAULT_QUERY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// Helper: read the active query timeout, falling back to
/// [`DEFAULT_QUERY_TIMEOUT`]. Backends call this once per
/// `execute_*_raw` invocation.
pub fn query_timeout_or_default() -> std::time::Duration {
    GRAPH_QUERY_TIMEOUT
        .try_with(|t| *t)
        .unwrap_or(DEFAULT_QUERY_TIMEOUT)
}

use ox_ontology::graph_exploration::{GraphSchemaOverview, NodeExpansion, SearchResultNode};
use ox_query_ir::query::QueryResult;
use ox_core::types::PropertyValue;

// ---------------------------------------------------------------------------
// TransienceDetector — backend-specific transient error classification
//
// Each GraphRuntime backend provides its own detector to decide which errors
// are worth retrying (network blips, leader switches) vs permanent (syntax
// errors, constraint violations).
// ---------------------------------------------------------------------------

/// Determines whether a graph database error is transient (worth retrying)
/// or permanent. Each GraphRuntime backend provides its own detection logic.
pub trait TransienceDetector: Send + Sync {
    /// Check if an error message indicates a transient failure.
    fn is_transient(&self, error_message: &str) -> bool;
}

// ---------------------------------------------------------------------------
// GraphRuntime trait — the execution boundary
//
// Each graph DB driver (Neo4j, Memgraph) implements this trait.
// Adding a new DB = implementing this trait + a GraphCompiler backend.
// ---------------------------------------------------------------------------

#[async_trait]
pub trait GraphRuntime: Send + Sync {
    /// Execute schema DDL statements (CREATE CONSTRAINT, CREATE INDEX, etc.)
    async fn execute_schema(&self, statements: &[String]) -> OxResult<()>;

    /// Backend-specific raw query execution. Receives the post-`pre_execute`
    /// query (with isolation predicates already injected) and returns the raw
    /// `QueryResult` before `post_execute` runs.
    ///
    /// Backends should override this. Callers should invoke
    /// [`execute_query`](Self::execute_query) instead, which runs the full
    /// pre → exec → post pipeline.
    async fn execute_query_raw(
        &self,
        query: &str,
        params: &HashMap<String, PropertyValue>,
    ) -> OxResult<QueryResult>;

    /// Execute a read query through the full pre → exec → post pipeline.
    ///
    /// Default impl: `pre_execute` → `execute_query_raw` → `post_execute`.
    /// Backends should NOT override this; instead override the hook methods.
    async fn execute_query(
        &self,
        query: &str,
        params: &HashMap<String, PropertyValue>,
    ) -> OxResult<QueryResult> {
        let (scoped_query, scoped_params) = self.pre_execute(query, params)?;
        let result = self
            .execute_query_raw(&scoped_query, &scoped_params)
            .await?;
        self.post_execute(&scoped_query, result).await
    }

    /// Pre-process a query before execution (default: identity).
    ///
    /// Backends override this to run the validator → rewriter → validator
    /// pipeline (safety + ontology gate, workspace-scope rewrite, post-rewrite
    /// scope gate). A validation failure returns `OxError::Validation` with
    /// every issue aggregated into one message so the caller (agent / LLM)
    /// can fix them in a single retry.
    fn pre_execute(
        &self,
        query: &str,
        params: &HashMap<String, PropertyValue>,
    ) -> OxResult<(String, HashMap<String, PropertyValue>)> {
        Ok((query.to_string(), params.clone()))
    }

    /// Post-process a query result (default: identity).
    ///
    /// Backends override this for audit logging, result enrichment, or any
    /// other after-the-fact transformation.
    async fn post_execute(&self, _query: &str, result: QueryResult) -> OxResult<QueryResult> {
        Ok(result)
    }

    /// Backend-specific raw bulk load. Receives the post-`pre_execute`
    /// query (with isolation predicates already injected) and the
    /// scope parameter map (e.g. `$_ws_id`) that must be bound to every
    /// per-record query.
    ///
    /// Backends should override this. Callers should invoke
    /// [`execute_load`](Self::execute_load) instead, which runs the full
    /// pre → exec pipeline so workspace isolation cannot be bypassed.
    async fn execute_load_raw(
        &self,
        query: &str,
        scope_params: &HashMap<String, PropertyValue>,
        batch: LoadBatch,
    ) -> OxResult<LoadResult>;

    /// Execute a batch load through the workspace-isolation pipeline.
    ///
    /// Default impl: `pre_execute` rewrites the query and produces the
    /// scope parameter map, then `execute_load_raw` does the work. A
    /// new backend that implements only `execute_load_raw` automatically
    /// gets workspace isolation — this is what closes the previous
    /// "forgot to call scope_query" risk.
    ///
    /// Backends MUST NOT override this; override `execute_load_raw`
    /// (and optionally `pre_execute`) instead.
    async fn execute_load(&self, query: &str, batch: LoadBatch) -> OxResult<LoadResult> {
        let (scoped_query, scope_params) = self.pre_execute(query, &HashMap::new())?;
        self.execute_load_raw(&scoped_query, &scope_params, batch)
            .await
    }

    /// Create an isolated sandbox namespace for test data
    async fn create_sandbox(&self, name: &str) -> OxResult<SandboxHandle>;

    /// Drop a sandbox and all its data
    async fn drop_sandbox(&self, handle: &SandboxHandle) -> OxResult<()>;

    /// Return the runtime name (for error messages and logging)
    fn name(&self) -> &str;

    /// Check if the runtime is reachable (with timeout)
    async fn health_check(&self) -> bool;

    // ---- Graph exploration (default: unsupported) ----

    /// Search nodes by text matching across properties.
    /// Labels filter restricts results to nodes with matching labels.
    async fn search_nodes(
        &self,
        _query: &str,
        _limit: usize,
        _labels: Option<&[String]>,
    ) -> OxResult<Vec<SearchResultNode>> {
        Err(OxError::UnsupportedOperation {
            target: self.name().to_string(),
            operation: "search_nodes".to_string(),
        })
    }

    /// Expand a node's 1-hop neighborhood.
    async fn expand_node(&self, _element_id: &str, _limit: usize) -> OxResult<NodeExpansion> {
        Err(OxError::UnsupportedOperation {
            target: self.name().to_string(),
            operation: "expand_node".to_string(),
        })
    }

    /// Get graph schema overview (label counts + relationship patterns).
    async fn graph_overview(&self) -> OxResult<GraphSchemaOverview> {
        Err(OxError::UnsupportedOperation {
            target: self.name().to_string(),
            operation: "graph_overview".to_string(),
        })
    }
}

// ---------------------------------------------------------------------------
// LoadBatch — validated batch of records for graph loading
// ---------------------------------------------------------------------------

/// Validated batch of records for graph loading.
/// Each record must be a JSON object (not array, string, etc.).
#[derive(Debug, Clone)]
pub struct LoadBatch {
    records: Vec<serde_json::Map<String, serde_json::Value>>,
}

impl LoadBatch {
    /// Validate and construct a `LoadBatch` from raw JSON values.
    /// Returns an error if any value is not a JSON object.
    pub fn from_values(values: Vec<serde_json::Value>) -> OxResult<Self> {
        let mut records = Vec::with_capacity(values.len());
        for (i, value) in values.into_iter().enumerate() {
            match value {
                serde_json::Value::Object(map) => records.push(map),
                other => {
                    return Err(OxError::Validation {
                        field: format!("batch[{i}]"),
                        message: format!("Expected JSON object, got {}", value_type_name(&other)),
                    });
                }
            }
        }
        Ok(Self { records })
    }

    pub fn records(&self) -> &[serde_json::Map<String, serde_json::Value>] {
        &self.records
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub fn into_records(self) -> Vec<serde_json::Map<String, serde_json::Value>> {
        self.records
    }
}

fn value_type_name(v: &serde_json::Value) -> &'static str {
    match v {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "boolean",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}

// ---------------------------------------------------------------------------
// Result types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize)]
pub struct LoadResult {
    pub nodes_created: usize,
    pub nodes_updated: usize,
    pub edges_created: usize,
    pub edges_updated: usize,
    pub batches_processed: usize,
    pub batches_failed: usize,
    pub errors: Vec<LoadError>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct LoadError {
    pub batch_index: usize,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct SandboxHandle {
    pub name: String,
    pub database: String,
}

#[cfg(test)]
mod load_batch_tests {
    use super::*;
    use ox_core::error::OxError;
    use serde_json::json;

    #[test]
    fn from_values_accepts_objects() {
        let values = vec![
            json!({"name": "Alice", "age": 30}),
            json!({"name": "Bob", "age": 25}),
        ];
        let batch = LoadBatch::from_values(values).expect("valid objects should be accepted");
        assert_eq!(batch.len(), 2);
        assert!(!batch.is_empty());

        let records = batch.records();
        assert_eq!(records[0].get("name").unwrap(), "Alice");
        assert_eq!(records[1].get("name").unwrap(), "Bob");
    }

    #[test]
    fn from_values_rejects_non_objects() {
        for (value, kind) in [
            (json!([1, 2, 3]), "array"),
            (json!("just a string"), "string"),
            (json!(null), "null"),
            (json!(42), "number"),
            (json!(true), "boolean"),
        ] {
            let err = LoadBatch::from_values(vec![value]).unwrap_err();
            match err {
                OxError::Validation { field, message } => {
                    assert_eq!(field, "batch[0]");
                    assert!(
                        message.contains(kind),
                        "message should mention '{kind}': {message}"
                    );
                }
                other => panic!("Expected Validation error, got {other:?}"),
            }
        }

        let values = vec![json!({"valid": true}), json!("invalid")];
        let err = LoadBatch::from_values(values).unwrap_err();
        match err {
            OxError::Validation { field, .. } => {
                assert_eq!(field, "batch[1]");
            }
            other => panic!("Expected Validation error, got {other:?}"),
        }
    }

    #[test]
    fn empty_vec_is_ok() {
        let batch = LoadBatch::from_values(vec![]).expect("empty vec should be valid");
        assert!(batch.is_empty());
        assert_eq!(batch.len(), 0);
        assert!(batch.records().is_empty());
    }

    #[test]
    fn into_records_yields_inserted() {
        let values = vec![json!({"x": 1}), json!({"y": 2})];
        let batch = LoadBatch::from_values(values).unwrap();
        let records = batch.into_records();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].get("x").unwrap(), 1);
        assert_eq!(records[1].get("y").unwrap(), 2);
    }
}

#[cfg(test)]
mod query_timeout_tests {
    use super::*;

    #[test]
    fn unset_timeout_falls_back_to_default() {
        // Outside any sync_scope — try_with returns Err, helper
        // returns the constant.
        assert_eq!(query_timeout_or_default(), DEFAULT_QUERY_TIMEOUT);
    }

    #[test]
    fn bound_timeout_overrides_default() {
        let custom = std::time::Duration::from_secs(5);
        let read = GRAPH_QUERY_TIMEOUT.sync_scope(custom, query_timeout_or_default);
        assert_eq!(read, custom);
    }

    #[test]
    fn nested_scope_uses_innermost_value() {
        let outer = std::time::Duration::from_secs(60);
        let inner = std::time::Duration::from_secs(2);
        let read = GRAPH_QUERY_TIMEOUT.sync_scope(outer, || {
            GRAPH_QUERY_TIMEOUT.sync_scope(inner, query_timeout_or_default)
        });
        assert_eq!(read, inner);
    }
}
