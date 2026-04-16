use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::types::{PropertyType, PropertyValue, deserialize_optional_property_value};

// ---------------------------------------------------------------------------
// Type-safe entity ID newtypes
//
// Prevent accidental mixing of node/edge/property/constraint IDs.
// Serialized as plain strings (serde transparent), so JSON format is unchanged.
// ---------------------------------------------------------------------------

macro_rules! entity_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
        #[serde(transparent)]
        pub struct $name(pub String);

        impl $name {
            pub fn new(s: impl Into<String>) -> Self { Self(s.into()) }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl std::ops::Deref for $name {
            type Target = str;
            fn deref(&self) -> &str { &self.0 }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str { &self.0 }
        }

        impl From<String> for $name {
            fn from(s: String) -> Self { Self(s) }
        }

        impl From<&str> for $name {
            fn from(s: &str) -> Self { Self(s.to_string()) }
        }

        impl PartialEq<str> for $name {
            fn eq(&self, other: &str) -> bool { self.0 == other }
        }

        impl PartialEq<&str> for $name {
            fn eq(&self, other: &&str) -> bool { self.0 == *other }
        }

        impl PartialEq<String> for $name {
            fn eq(&self, other: &String) -> bool { self.0 == *other }
        }

        impl std::borrow::Borrow<str> for $name {
            fn borrow(&self) -> &str { &self.0 }
        }
    };
}

entity_id!(
    /// Type-safe identifier for node types in an ontology.
    NodeTypeId
);
entity_id!(
    /// Type-safe identifier for edge types in an ontology.
    EdgeTypeId
);
entity_id!(
    /// Type-safe identifier for property definitions.
    PropertyId
);
entity_id!(
    /// Type-safe identifier for constraint definitions.
    ConstraintId
);

// ---------------------------------------------------------------------------
// OntologyVersion — temporal version metadata
// ---------------------------------------------------------------------------

/// Version metadata for a point-in-time ontology snapshot.
///
/// `number` is monotonically increasing and is the primary comparator.
/// The remaining fields provide temporal and provenance context:
/// - `valid_from` / `valid_to`: the window during which this version
///   was the active schema (used by `as_of` queries).
/// - `committed_by` / `commit_message`: audit trail.
///
/// Implements `From<u32>` so that callers can pass a bare version
/// number and get a zero-metadata instance — preserving compatibility
/// with the original `version: u32` API surface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct OntologyVersion {
    pub number: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_from: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_to: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub committed_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit_message: Option<String>,
}

impl Default for OntologyVersion {
    fn default() -> Self {
        Self {
            number: 1,
            valid_from: None,
            valid_to: None,
            committed_by: None,
            commit_message: None,
        }
    }
}

impl From<u32> for OntologyVersion {
    fn from(n: u32) -> Self {
        Self { number: n, ..Default::default() }
    }
}

impl std::fmt::Display for OntologyVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "v{}", self.number)
    }
}

// ---------------------------------------------------------------------------
// NodeTypeDef
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct NodeTypeDef {
    /// Stable UUID for this node type
    pub id: NodeTypeId,
    /// Label name (e.g. "Product", "Customer")
    pub label: String,
    /// Optional human-readable description
    pub description: Option<String>,
    /// Source table this node was derived from (for DB sources)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_table: Option<String>,
    /// Properties on this node type
    #[serde(default)]
    pub properties: Vec<PropertyDef>,
    /// Constraints on this node type
    #[serde(default)]
    pub constraints: Vec<ConstraintDef>,
    /// Parent node type for inheritance (e.g., Employee is-a Person).
    pub parent: Option<NodeTypeId>,
    /// Governance metadata (owner, steward, tags, retention policy).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub governance: Option<Governance>,
    /// Detailed source lineage (richer than the legacy `source_table` field).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_lineage: Option<SourceLineage>,
}

/// Type-safe reference to the owner of a property — node or edge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PropertyOwner {
    Node(NodeTypeId),
    Edge(EdgeTypeId),
}

impl PropertyOwner {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Node(id) => id.as_ref(),
            Self::Edge(id) => id.as_ref(),
        }
    }
}

impl std::fmt::Display for PropertyOwner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Node(id) => write!(f, "node:{id}"),
            Self::Edge(id) => write!(f, "edge:{id}"),
        }
    }
}

/// Tracks which external data source a node type was derived from.
/// Richer than the legacy `source_table: Option<String>` — includes
/// composite primary key and source system identifier for impact analysis.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SourceLineage {
    /// Registered data source ID (matches `ox-source` registry key).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
    /// Source table or collection name.
    pub table: String,
    /// Primary key columns in the source table.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub primary_key: Vec<String>,
}

/// Governance metadata attached to a node type.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Governance {
    /// Principal ID of the business owner responsible for this entity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_principal: Option<String>,
    /// Principal ID of the data steward (operational contact).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub steward: Option<String>,
    /// Free-form tags for classification (e.g., "core", "legal", "deprecated").
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Data retention period in days. `None` = no automatic deletion.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retention_days: Option<u32>,
}

impl Default for NodeTypeDef {
    fn default() -> Self {
        Self {
            id: NodeTypeId::default(),
            label: String::new(),
            description: None,
            source_table: None,
            properties: vec![],
            constraints: vec![],
            parent: None,
            governance: None,
            source_lineage: None,
        }
    }
}

impl NodeTypeDef {
    pub fn required_properties(&self) -> impl Iterator<Item = &PropertyDef> {
        self.properties.iter().filter(|p| !p.nullable)
    }

    pub fn has_unique_constraint(&self) -> bool {
        self.constraints
            .iter()
            .any(|c| matches!(c.constraint, NodeConstraint::Unique { .. }))
    }
}

// ---------------------------------------------------------------------------
// EdgeTypeDef
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EdgeTypeDef {
    /// Stable UUID for this edge type
    pub id: EdgeTypeId,
    /// Relationship label (e.g. "PURCHASED", "REVIEWED")
    pub label: String,
    /// Optional human-readable description
    pub description: Option<String>,
    /// Source node type ID (references NodeTypeDef.id)
    pub source_node_id: NodeTypeId,
    /// Target node type ID (references NodeTypeDef.id)
    pub target_node_id: NodeTypeId,
    /// Properties on this edge type
    #[serde(default)]
    pub properties: Vec<PropertyDef>,
    /// Cardinality constraint
    #[serde(default = "default_cardinality")]
    pub cardinality: Cardinality,
    /// Logical inverse edge (e.g., PURCHASED ↔ PURCHASED_BY).
    pub inverse_of: Option<EdgeTypeId>,
}

impl Default for EdgeTypeDef {
    fn default() -> Self {
        Self {
            id: EdgeTypeId::default(),
            label: String::new(),
            description: None,
            source_node_id: NodeTypeId::default(),
            target_node_id: NodeTypeId::default(),
            properties: vec![],
            cardinality: Cardinality::ManyToMany,
            inverse_of: None,
        }
    }
}

// ---------------------------------------------------------------------------
// DataClassification — sensitivity level for a property
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DataClassification {
    Public,
    Internal,
    Confidential,
    Restricted,
}

impl std::fmt::Display for DataClassification {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Public => f.write_str("public"),
            Self::Internal => f.write_str("internal"),
            Self::Confidential => f.write_str("confidential"),
            Self::Restricted => f.write_str("restricted"),
        }
    }
}

// ---------------------------------------------------------------------------
// PropertyDef — a single property on a node or edge
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PropertyDef {
    /// Stable UUID for this property
    pub id: PropertyId,
    /// Property name (e.g. "name", "price", "created_at")
    pub name: String,
    /// Data type
    pub property_type: PropertyType,
    /// Whether this property can be null
    #[serde(default)]
    pub nullable: bool,
    /// Default value if not provided
    #[serde(default, deserialize_with = "deserialize_optional_property_value")]
    pub default_value: Option<PropertyValue>,
    /// Human-readable description
    pub description: Option<String>,
    /// Data sensitivity classification (derived from PII detection)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub classification: Option<DataClassification>,
    /// Semantic type hint (Email, Phone, Url, etc.) for richer LLM context.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub semantic_type: Option<SemanticType>,
    /// Physical unit for numeric properties (e.g., "USD", "kg", "seconds").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    /// PII kind — user-declared, never auto-assigned. See Phase 4.6.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pii_kind: Option<PiiKind>,
    /// Source column name this property was derived from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_column: Option<String>,
    /// Transformation expression applied to the source column (e.g., `UPPER(col)`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform: Option<String>,
}

impl Default for PropertyDef {
    fn default() -> Self {
        Self {
            id: PropertyId::default(),
            name: String::new(),
            property_type: PropertyType::String,
            nullable: false,
            default_value: None,
            description: None,
            classification: None,
            semantic_type: None,
            unit: None,
            pii_kind: None,
            source_column: None,
            transform: None,
        }
    }
}

/// Semantic type of a property value — higher-level than PropertyType.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SemanticType {
    Email,
    Phone,
    Url,
    Address,
    Coordinate,
    Currency,
    Percentage,
    Iso8601,
}

/// PII (Personally Identifiable Information) classification.
/// Set explicitly by the user via UI confirmation, never auto-assigned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PiiKind {
    Name,
    Email,
    Phone,
    Ssn,
    CreditCard,
    Address,
    DateOfBirth,
    IpAddress,
    Other,
}

// ---------------------------------------------------------------------------
// Cardinality — relationship multiplicity
// ---------------------------------------------------------------------------

fn default_cardinality() -> Cardinality {
    Cardinality::ManyToMany
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Cardinality {
    OneToOne,
    OneToMany,
    ManyToOne,
    ManyToMany,
}

// ---------------------------------------------------------------------------
// ConstraintDef — wrapper with stable ID around NodeConstraint
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ConstraintDef {
    /// Stable UUID for this constraint
    pub id: ConstraintId,
    /// The constraint definition
    #[serde(flatten)]
    pub constraint: NodeConstraint,
}

// ---------------------------------------------------------------------------
// NodeConstraint — structural constraint on a node type
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NodeConstraint {
    /// Properties must be unique across all nodes of this type
    Unique { property_ids: Vec<PropertyId> },
    /// Property must exist (NOT NULL at DB level)
    Exists { property_id: PropertyId },
    /// Composite key — combination of properties is unique and required
    NodeKey { property_ids: Vec<PropertyId> },
}

// ---------------------------------------------------------------------------
// IndexDef — index for query performance
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum IndexDef {
    /// Single-property index
    Single {
        id: String,
        node_id: NodeTypeId,
        property_id: PropertyId,
    },
    /// Composite index on multiple properties
    Composite {
        id: String,
        node_id: NodeTypeId,
        property_ids: Vec<PropertyId>,
    },
    /// Full-text search index
    FullText {
        id: String,
        name: String,
        node_id: NodeTypeId,
        property_ids: Vec<PropertyId>,
    },
    /// Vector index for similarity search (future: pgvector, Neo4j vector)
    Vector {
        id: String,
        node_id: NodeTypeId,
        property_id: PropertyId,
        dimensions: usize,
        similarity: VectorSimilarity,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum VectorSimilarity {
    Cosine,
    Euclidean,
}
