use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::i18n::LocalizedText;
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
        Self {
            number: n,
            ..Default::default()
        }
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

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct NodeTypeDef {
    /// Stable UUID for this node type.
    pub id: NodeTypeId,
    /// Canonical, language-neutral label used as the Neo4j node label and in
    /// query identifiers. Must satisfy [`crate::types::is_valid_graph_identifier`].
    pub label: String,
    /// Localized display name shown in the UI. Defaults to empty; consumers
    /// typically fall back to `label` when the display name is empty.
    #[serde(default)]
    pub display_name: LocalizedText,
    /// Localized human-readable description.
    #[serde(default)]
    pub description: LocalizedText,
    /// Properties on this node type.
    #[serde(default)]
    pub properties: Vec<PropertyDef>,
    /// Constraints on this node type.
    #[serde(default)]
    pub constraints: Vec<ConstraintDef>,
    /// Parent node type for inheritance (e.g., Employee is-a Person).
    pub parent: Option<NodeTypeId>,
    /// Governance metadata (owner, steward, tags, retention policy).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub governance: Option<Governance>,
    /// Source lineage — which data source table this node was derived from
    /// (table name, primary key, source system). Authoritative replacement
    /// for the removed `source_table` field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_lineage: Option<SourceLineage>,
    /// Deprecation timestamp. When set, the node is marked for removal and
    /// UI consumers should render it with a deprecated indicator. Queries
    /// still work for compatibility until the deprecation window elapses.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deprecated_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Points at the replacement entity when this node is deprecated. Used
    /// by the UI to guide users from deprecated to current entities.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replaced_by_id: Option<NodeTypeId>,
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

/// A suggested PII classification awaiting user confirmation.
///
/// Auto-detection (Phase 4.6) produces suggestions — never sets `pii_kind`
/// directly on PropertyDef. The user confirms or rejects each suggestion
/// via the UI before it becomes a committed classification.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PiiSuggestion {
    /// Property affected.
    pub property_id: PropertyId,
    /// Node or edge owning the property.
    pub owner_label: String,
    /// Suggested PII kind.
    pub suggested_kind: PiiKind,
    /// Confidence score (0.0 – 1.0).
    pub confidence: f64,
    /// Evidence that led to the suggestion (e.g., "column name contains 'email'").
    pub evidence: String,
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
    /// Stable UUID for this edge type.
    pub id: EdgeTypeId,
    /// Canonical, language-neutral relationship label (e.g. "PURCHASED",
    /// "REVIEWED"). Used as the Neo4j relationship type.
    pub label: String,
    /// Localized display name shown in the UI.
    #[serde(default)]
    pub display_name: LocalizedText,
    /// Localized human-readable description.
    #[serde(default)]
    pub description: LocalizedText,
    /// Source node type ID (references NodeTypeDef.id).
    pub source_node_id: NodeTypeId,
    /// Target node type ID (references NodeTypeDef.id).
    pub target_node_id: NodeTypeId,
    /// Role played by the source endpoint, e.g. "manager" for a MANAGES edge
    /// from Employee to Employee. Distinguishes the *functional* role from
    /// the edge label itself when the same relationship label could carry
    /// different semantics depending on direction.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_role: Option<String>,
    /// Role played by the target endpoint (e.g. "direct_report").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_role: Option<String>,
    /// Properties on this edge type.
    #[serde(default)]
    pub properties: Vec<PropertyDef>,
    /// Cardinality constraint.
    #[serde(default = "default_cardinality")]
    pub cardinality: Cardinality,
    /// Logical inverse edge (e.g., PURCHASED ↔ PURCHASED_BY).
    pub inverse_of: Option<EdgeTypeId>,
    /// Free-form tags (e.g. "i18n", "derived", "temporal") for downstream
    /// filtering and UI grouping. Not validated; ontology designer's choice.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Deprecation timestamp. See [`NodeTypeDef::deprecated_at`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deprecated_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Replacement edge when deprecated. See [`NodeTypeDef::replaced_by_id`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replaced_by_id: Option<EdgeTypeId>,
}

impl Default for EdgeTypeDef {
    fn default() -> Self {
        Self {
            id: EdgeTypeId::default(),
            label: String::new(),
            display_name: LocalizedText::default(),
            description: LocalizedText::default(),
            source_node_id: NodeTypeId::default(),
            target_node_id: NodeTypeId::default(),
            source_role: None,
            target_role: None,
            properties: vec![],
            cardinality: Cardinality::ManyToMany,
            inverse_of: None,
            tags: Vec::new(),
            deprecated_at: None,
            replaced_by_id: None,
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
    /// Stable UUID for this property.
    pub id: PropertyId,
    /// Canonical, language-neutral property name (e.g. "name", "price",
    /// "created_at"). Used as the Neo4j property key.
    pub name: String,
    /// Localized display name shown in the UI.
    #[serde(default)]
    pub display_name: LocalizedText,
    /// Data type.
    pub property_type: PropertyType,
    /// Whether this property can be null.
    #[serde(default)]
    pub nullable: bool,
    /// Default value if not provided.
    #[serde(default, deserialize_with = "deserialize_optional_property_value")]
    pub default_value: Option<PropertyValue>,
    /// Localized human-readable description.
    #[serde(default)]
    pub description: LocalizedText,
    /// Minimum cardinality for list-valued properties (inclusive). `None`
    /// means unbounded at the lower end (i.e., the list can be empty when
    /// `nullable` is also permissive). Maps to `owl:minCardinality` and
    /// SHACL `sh:minCount` on export.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_count: Option<u32>,
    /// Maximum cardinality for list-valued properties (inclusive). `None`
    /// means unbounded. Maps to `owl:maxCardinality` and SHACL `sh:maxCount`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_count: Option<u32>,
    /// Marks the property as storing a localized value. The expected runtime
    /// shape is `PropertyType::Map` with `{locale: value}` entries, or a
    /// structured `LocalizedText` document. UI and LLM consumers use this
    /// hint to filter or merge translations based on the caller's locale.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_localized: bool,
    /// Data sensitivity classification (derived from PII detection).
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
    /// Deprecation timestamp. See [`NodeTypeDef::deprecated_at`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deprecated_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Replacement property when deprecated. See [`NodeTypeDef::replaced_by_id`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replaced_by_id: Option<PropertyId>,
}

impl Default for PropertyDef {
    fn default() -> Self {
        Self {
            id: PropertyId::default(),
            name: String::new(),
            display_name: LocalizedText::default(),
            property_type: PropertyType::String,
            nullable: false,
            default_value: None,
            description: LocalizedText::default(),
            min_count: None,
            max_count: None,
            is_localized: false,
            classification: None,
            semantic_type: None,
            unit: None,
            pii_kind: None,
            source_column: None,
            transform: None,
            deprecated_at: None,
            replaced_by_id: None,
        }
    }
}

/// Semantic type of a property value — higher-level than PropertyType.
///
/// Canonical variants cover globally common semantics. `LocalizedText` marks
/// a property whose runtime value is a `{locale: text}` map (pairs with
/// `PropertyDef::is_localized`). `Other(String)` is an open extension point
/// for domain-specific semantics (e.g. "ISBN", "VIN", "CUSIP") that the
/// platform does not want to hardcode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum SemanticType {
    Email,
    Phone,
    Url,
    Address,
    Coordinate,
    Currency,
    Percentage,
    Iso8601,
    /// Value is a localized text map. Requires `PropertyDef::is_localized == true`.
    LocalizedText,
    /// Open extension: caller-supplied semantic identifier (e.g. "ISBN", "VIN").
    Other(String),
}

/// PII (Personally Identifiable Information) classification.
/// Set explicitly by the user via UI confirmation, never auto-assigned.
///
/// Covers the union of GDPR Article 4(1), HIPAA Safe Harbor identifiers,
/// PCI DSS cardholder data, and ICAO / national-identity schemes. `NationalId`
/// is parameterised by ISO 3166-1 alpha-2 country code so country-specific
/// schemes (KR RRN, US SSN, JP My Number, etc.) stay distinguishable in
/// downstream masking rules. `Custom` is the escape hatch for schemes the
/// platform has not anticipated — still tracked as PII for audit purposes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum PiiKind {
    // --- Identity ---
    Name,
    DateOfBirth,
    /// National identifier scoped by ISO 3166-1 alpha-2 country code
    /// (e.g. `{"kind":"national_id","value":"kr"}`).
    NationalId {
        country: String,
    },
    Passport,
    DriversLicense,
    // --- Contact ---
    Email,
    Phone,
    Address,
    IpAddress,
    // --- Financial / PCI DSS ---
    PaymentCardNumber,
    BankAccountNumber,
    Iban,
    /// Historical variant kept for backfill paths; prefer `PaymentCardNumber`.
    CreditCard,
    /// Historical variant kept for backfill paths; prefer `NationalId { country }`.
    Ssn,
    // --- Health / HIPAA ---
    MedicalRecordNumber,
    InsuranceId,
    // --- Biometric / Location ---
    Biometric,
    GeoLocation,
    // --- Open extension ---
    /// Caller-supplied PII scheme name for anything the platform has not
    /// predefined. Still treated as PII for audit and masking purposes.
    Custom(String),
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
