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
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
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
