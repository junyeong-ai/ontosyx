pub mod enrichment;
pub mod isolation;
pub mod neo4j;
pub mod profiler;
pub mod registry;

use std::collections::HashMap;

use async_trait::async_trait;
use uuid::Uuid;

use ox_core::error::{OxError, OxResult};

// ---------------------------------------------------------------------------
// Per-request graph workspace context via task-local
// ---------------------------------------------------------------------------
// Set by the workspace middleware. Read by Neo4jRuntime to scope graph queries.
// Mirrors ox_store::WORKSPACE_ID / SYSTEM_BYPASS for the graph layer.
// ---------------------------------------------------------------------------

tokio::task_local! {
    /// Per-request workspace ID for graph isolation.
    pub static GRAPH_WORKSPACE_ID: Uuid;
    /// When true, graph queries bypass workspace isolation (system tasks).
    pub static GRAPH_SYSTEM_BYPASS: bool;
}
use ox_core::graph_exploration::{GraphSchemaOverview, NodeExpansion, SearchResultNode};
use ox_core::query_ir::QueryResult;
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
// Each graph DB driver (Neo4j, Neptune, etc.) implements this trait.
// Adding a new DB = implementing this trait + a GraphCompiler backend.
// ---------------------------------------------------------------------------

#[async_trait]
pub trait GraphRuntime: Send + Sync {
    /// Execute schema DDL statements (CREATE CONSTRAINT, CREATE INDEX, etc.)
    async fn execute_schema(&self, statements: &[String]) -> OxResult<()>;

    /// Execute a read query and return results
    async fn execute_query(
        &self,
        query: &str,
        params: &HashMap<String, PropertyValue>,
    ) -> OxResult<QueryResult>;

    /// Execute a batch load with validated records
    async fn execute_load(&self, query: &str, batch: LoadBatch) -> OxResult<LoadResult>;

    /// Create an isolated sandbox namespace for test data
    async fn create_sandbox(&self, name: &str) -> OxResult<SandboxHandle>;

    /// Drop a sandbox and all its data
    async fn drop_sandbox(&self, handle: &SandboxHandle) -> OxResult<()>;

    /// Return the runtime name (for error messages and logging)
    fn runtime_name(&self) -> &str;

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
            target: self.runtime_name().to_string(),
            operation: "search_nodes".to_string(),
        })
    }

    /// Expand a node's 1-hop neighborhood.
    async fn expand_node(&self, _element_id: &str, _limit: usize) -> OxResult<NodeExpansion> {
        Err(OxError::UnsupportedOperation {
            target: self.runtime_name().to_string(),
            operation: "expand_node".to_string(),
        })
    }

    /// Get graph schema overview (label counts + relationship patterns).
    async fn graph_overview(&self) -> OxResult<GraphSchemaOverview> {
        Err(OxError::UnsupportedOperation {
            target: self.runtime_name().to_string(),
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
