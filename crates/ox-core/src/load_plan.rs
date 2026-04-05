use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::types::PropertyType;

// ---------------------------------------------------------------------------
// LoadMode — full vs. incremental (watermark-based) loading
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum LoadMode {
    /// Replace all data (default behavior).
    #[default]
    Full,
    /// Only load records newer than the last checkpoint.
    Incremental {
        /// Column to use as watermark (e.g., "updated_at", "id").
        watermark_column: String,
    },
}

// ---------------------------------------------------------------------------
// LoadPlan — DB-agnostic data loading strategy
//
// Describes HOW to load data from a source (CSV, JSON, RDB) into a graph,
// without any reference to specific query syntax.
//
// Compiles to:
//   Neo4j   → UNWIND $batch AS row MERGE (n:Label {key: row.key}) SET ...
//   Neptune → openCypher MERGE or Gremlin addV/addE
//   Export  → Airflow DAG, dbt model, raw script
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LoadPlan {
    /// Unique identifier
    pub id: String,
    /// Reference to the OntologyIR this plan targets
    pub ontology_id: String,
    /// Ontology version this plan was generated for
    pub ontology_version: u32,
    /// Description of the data source
    pub source: DataSourceSpec,
    /// Ordered list of load steps (respects dependencies)
    pub steps: Vec<LoadStep>,
    /// Batch execution configuration
    pub batch_config: BatchConfig,
    /// Loading mode: full replacement or incremental (watermark-based).
    #[serde(default)]
    pub mode: LoadMode,
}

// ---------------------------------------------------------------------------
// DataSourceSpec — describes the shape of incoming data
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(tag = "format", rename_all = "snake_case")]
pub enum DataSourceSpec {
    /// CSV file source
    Csv {
        /// Column delimiter
        delimiter: char,
        /// Whether the first row is a header
        has_header: bool,
        /// Detected/declared columns
        columns: Vec<ColumnSpec>,
    },

    /// JSON file source
    Json {
        /// JSONPath to the array of records (e.g. "$.data[*]")
        root_path: Option<String>,
        /// Detected/declared fields
        fields: Vec<ColumnSpec>,
    },

    /// Relational database source (DDL-based)
    Relational {
        /// Table name
        table_name: String,
        /// Columns
        columns: Vec<ColumnSpec>,
    },
}

/// Custom deserializer: accepts both `"format"` and `"type"` as discriminator keys.
/// LLMs often generate `{"type": "csv"}` instead of `{"format": "csv"}`, or
/// invent types like `"multi_file"` — these are normalized to Csv with defaults.
impl<'de> Deserialize<'de> for DataSourceSpec {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        let obj = value
            .as_object()
            .ok_or_else(|| serde::de::Error::custom("expected object for DataSourceSpec"))?;

        // Accept "format" or "type" as discriminator
        let format = obj
            .get("format")
            .or_else(|| obj.get("type"))
            .and_then(|v| v.as_str())
            .unwrap_or("csv");

        match format {
            "csv" => {
                let delimiter = obj
                    .get("delimiter")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.chars().next())
                    .unwrap_or(',');
                let has_header = obj
                    .get("has_header")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                let columns = obj
                    .get("columns")
                    .and_then(|v| serde_json::from_value::<Vec<ColumnSpec>>(v.clone()).ok())
                    .unwrap_or_default();
                Ok(DataSourceSpec::Csv {
                    delimiter,
                    has_header,
                    columns,
                })
            }
            "json" => {
                let root_path = obj
                    .get("root_path")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let fields = obj
                    .get("fields")
                    .and_then(|v| serde_json::from_value::<Vec<ColumnSpec>>(v.clone()).ok())
                    .unwrap_or_default();
                Ok(DataSourceSpec::Json { root_path, fields })
            }
            "relational" => {
                let table_name = obj
                    .get("table_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let columns = obj
                    .get("columns")
                    .and_then(|v| serde_json::from_value::<Vec<ColumnSpec>>(v.clone()).ok())
                    .unwrap_or_default();
                Ok(DataSourceSpec::Relational {
                    table_name,
                    columns,
                })
            }
            // Unknown format: default to CSV
            _ => {
                // Unknown format — fall back to CSV with defaults
                Ok(DataSourceSpec::Csv {
                    delimiter: ',',
                    has_header: true,
                    columns: vec![],
                })
            }
        }
    }
}

/// A single column/field in the source data
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ColumnSpec {
    /// Column name as it appears in the source
    pub name: String,
    /// Inferred or declared data type
    pub inferred_type: PropertyType,
    /// Sample values (for LLM analysis / dry-run preview)
    pub sample_values: Vec<String>,
    /// Whether this column contains null values
    pub has_nulls: bool,
}

// ---------------------------------------------------------------------------
// LoadStep — a single step in the load plan
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LoadStep {
    /// Execution order (0-based)
    pub order: u32,
    /// Steps that must complete before this one
    pub depends_on: Vec<u32>,
    /// The operation to perform
    pub operation: LoadOp,
    /// Human-readable description for dry-run display
    pub description: String,
}

// ---------------------------------------------------------------------------
// LoadOp — the actual load operation (DB-agnostic)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum LoadOp {
    /// Upsert nodes of a given type
    UpsertNode {
        /// Target node label in the ontology
        target_label: String,
        /// Fields used to MATCH existing nodes (identity)
        match_fields: Vec<PropertyMapping>,
        /// Fields to SET on both create and update
        set_fields: Vec<PropertyMapping>,
        /// What to do when a matching node already exists
        on_conflict: ConflictStrategy,
    },

    /// Upsert edges between two node types
    UpsertEdge {
        /// Target edge label in the ontology
        target_label: String,
        /// How to find the source node
        source_match: NodeMatch,
        /// How to find the target node
        target_match: NodeMatch,
        /// Properties to set on the edge
        set_fields: Vec<PropertyMapping>,
        /// What to do when a matching edge already exists
        on_conflict: ConflictStrategy,
    },
}

// ---------------------------------------------------------------------------
// PropertyMapping — maps a source column to a graph property
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PropertyMapping {
    /// Source column/field name
    pub source_column: String,
    /// Target property name in the graph
    pub graph_property: String,
    /// Optional transformation to apply
    pub transform: Option<Transform>,
}

/// Data transformation to apply during mapping
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "transform", rename_all = "snake_case")]
pub enum Transform {
    /// Convert to string
    ToString,
    /// Parse as integer
    ToInt,
    /// Parse as float
    ToFloat,
    /// Parse as boolean
    ToBool,
    /// Parse as date with given format (e.g. "%Y-%m-%d")
    ToDate { format: String },
    /// Parse as datetime with given format
    ToDateTime { format: String },
    /// Apply a trim operation
    Trim,
    /// Convert to lowercase
    ToLower,
    /// Convert to uppercase
    ToUpper,
    /// Split string and take nth element
    Split { delimiter: String, index: usize },
    /// Custom expression (evaluated at runtime)
    Custom { expression: String },
}

// ---------------------------------------------------------------------------
// NodeMatch — how to find an existing node for edge creation
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct NodeMatch {
    /// The node label to match
    pub label: String,
    /// The property on the node to match against
    pub match_property: String,
    /// The source field whose value is used for matching
    pub source_field: String,
}

// ---------------------------------------------------------------------------
// ConflictStrategy — what happens on duplicate
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConflictStrategy {
    /// Update existing properties with new values
    Update,
    /// Skip the row, keep existing data
    Skip,
    /// Raise an error
    Error,
    /// Merge: update only non-null new values
    MergeNonNull,
}

// ---------------------------------------------------------------------------
// BatchConfig — execution tuning
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct BatchConfig {
    /// Number of records per batch
    pub batch_size: usize,
    /// Number of parallel batches (for large loads)
    pub parallelism: usize,
    /// Whether to wrap each batch in a transaction
    pub transactional: bool,
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self {
            batch_size: 1000,
            parallelism: 1,
            transactional: true,
        }
    }
}

// ---------------------------------------------------------------------------
// DryRunResult — preview of what a LoadPlan would do
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DryRunResult {
    /// The compiled queries that would be executed
    pub compiled_queries: Vec<CompiledStep>,
    /// Total records in source
    pub total_records: usize,
    /// Estimated number of nodes to create/update
    pub estimated_nodes: usize,
    /// Estimated number of edges to create/update
    pub estimated_edges: usize,
    /// Any warnings (type mismatches, missing values, etc.)
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CompiledStep {
    pub step_order: u32,
    pub description: String,
    /// The actual compiled query in the target language
    pub query: String,
    /// Sample parameter values (first batch)
    pub sample_params: Option<serde_json::Value>,
}
