//! Graph exploration types — shared between API layer and runtime implementations.
//!
//! These types define the contract for graph browsing operations (search, expand, overview)
//! independent of the underlying graph database.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Node search
// ---------------------------------------------------------------------------

/// A node returned by a graph search operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResultNode {
    pub element_id: String,
    pub labels: Vec<String>,
    pub props: HashMap<String, serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Node expansion (1-hop neighbors)
// ---------------------------------------------------------------------------

/// A neighbor of an expanded node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpandNeighbor {
    pub element_id: String,
    pub labels: Vec<String>,
    pub props: HashMap<String, serde_json::Value>,
    pub relationship_type: String,
    /// "outgoing" or "incoming"
    pub direction: String,
}

/// Result of expanding a node's neighborhood.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeExpansion {
    pub source_id: String,
    pub neighbors: Vec<ExpandNeighbor>,
}

// ---------------------------------------------------------------------------
// Graph schema overview
// ---------------------------------------------------------------------------

/// Statistics for a single node label.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabelStat {
    pub label: String,
    pub count: i64,
}

/// A relationship pattern in the graph schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationshipPattern {
    pub from_label: String,
    pub rel_type: String,
    pub to_label: String,
    pub count: i64,
}

/// A property discovered from the graph schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertySchema {
    /// Node label or relationship type this property belongs to.
    pub entity_type: String,
    /// Property name (e.g., "name", "price").
    pub property_name: String,
    /// Neo4j type strings (e.g., ["STRING"], ["INTEGER"]).
    pub property_types: Vec<String>,
    /// Whether this property is mandatory (NOT NULL).
    pub mandatory: bool,
}

/// High-level overview of the graph schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphSchemaOverview {
    pub labels: Vec<LabelStat>,
    pub relationships: Vec<RelationshipPattern>,
    pub total_nodes: i64,
    pub total_relationships: i64,
    /// Property schema for node types (from db.schema.nodeTypeProperties).
    pub node_properties: Vec<PropertySchema>,
    /// Property schema for relationship types (from db.schema.relTypeProperties).
    pub rel_properties: Vec<PropertySchema>,
}
