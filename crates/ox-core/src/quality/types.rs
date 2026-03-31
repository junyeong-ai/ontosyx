use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Quality report types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OntologyQualityReport {
    pub confidence: QualityConfidence,
    /// Identified gaps that may reduce query accuracy or semantic correctness.
    /// Ordered by severity (high first).
    pub gaps: Vec<QualityGap>,
}

/// Overall confidence in the generated ontology's ability to support correct query generation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QualityConfidence {
    /// No significant gaps — ontology is ready for production use.
    High,
    /// Some gaps exist but queries will largely work. Refinement recommended.
    Medium,
    /// Significant gaps that will likely cause wrong query generation. Refine before use.
    Low,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityGap {
    pub severity: QualityGapSeverity,
    pub category: QualityGapCategory,
    /// Structured reference to the entity where the gap is located.
    pub location: QualityGapRef,
    /// What is unclear or potentially wrong
    pub issue: String,
    /// What additional information would resolve this gap
    pub suggestion: String,
}

/// Structured reference to the entity affected by a quality gap.
/// Carries entity IDs for direct graph canvas matching.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "ref_type", rename_all = "snake_case")]
pub enum QualityGapRef {
    Node {
        node_id: String,
        label: String,
    },
    NodeProperty {
        node_id: String,
        property_id: String,
        label: String,
        property_name: String,
    },
    Edge {
        edge_id: String,
        label: String,
    },
    EdgeProperty {
        edge_id: String,
        property_id: String,
        label: String,
        property_name: String,
    },
    SourceTable {
        table: String,
    },
    SourceColumn {
        table: String,
        column: String,
    },
    SourceForeignKey {
        from_table: String,
        from_column: String,
        to_table: String,
        to_column: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QualityGapSeverity {
    /// Will likely cause wrong query generation
    High,
    /// May cause confusion or suboptimal queries
    Medium,
    /// Minor improvement possible, queries will still work
    Low,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QualityGapCategory {
    /// A short/cryptic enum value whose meaning cannot be inferred from schema alone
    OpaqueEnumValue,
    /// All sample values are integers — likely a numeric status/type code whose meaning is unknown
    NumericEnumCode,
    /// Only one value was observed in sample data — may not reflect full production range
    SingleValueBias,
    /// Table has very few rows — statistics may not be representative
    SmallSample,
    /// A property or edge in the generated ontology has no description
    MissingDescription,
    /// Over 80% of values are null — property may be unused or conditionally populated
    SparseProperty,
    /// A source table/collection was not represented by any ontology node type
    UnmappedSourceTable,
    /// A declared source foreign key was not represented by any ontology edge
    MissingForeignKeyEdge,
    /// An inferred containment relationship (e.g., JSON nesting) was not represented by any ontology edge
    MissingContainmentEdge,
    /// A non-key source column has no corresponding ontology property on the mapped node
    UnmappedSourceColumn,
    /// Multiple edges between the same node pair with similar semantics — likely duplicates
    DuplicateEdge,
    /// Node type with no incoming or outgoing edges — disconnected from the graph
    OrphanNode,
    /// Same property name used with different types across node types
    PropertyTypeInconsistency,
    /// Node type with an unusually high number of edges — potential god-node
    HubNode,
    /// A property appearing on many different node types — may deserve its own node
    OverloadedProperty,
    /// Edge type where source and target are the same node type
    SelfReferentialEdge,
}

/// Returns true if the value is a short (1-2 char) uppercase code.
/// Whether it is truly "cryptic" depends on caller context (coexistence with longer values).
/// Exported for reuse in source analysis to avoid duplication.
pub fn is_cryptic_short(value: &str) -> bool {
    value.len() <= 2 && !value.is_empty() && value.chars().all(|c| c.is_ascii_uppercase())
}

pub(super) fn is_excluded(table_name: &str, excluded_tables: &[String]) -> bool {
    let lower = table_name.to_ascii_lowercase();
    excluded_tables.iter().any(|excluded| excluded == &lower)
}
