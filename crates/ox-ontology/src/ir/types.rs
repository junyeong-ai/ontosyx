use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use ox_core::graph_label::GraphLabel;
use ox_core::i18n::LocalizedText;
use ox_core::property_key::PropertyKey;
use ox_core::types::{PropertyType, PropertyValue, deserialize_optional_property_value};

// ---------------------------------------------------------------------------
// Type-safe entity ID newtypes
//
// Defined via the shared `define_id_newtype!` macro in `ox-core::id` so
// every crate in the workspace that introduces a new id type
// (Phase 3+: `MappingId`, `RuleId`, `ActionId`, `FunctionId`,
// `MetricId`, `GlossaryTermId`, ...) picks up the same trait surface
// without copying the boilerplate.
// ---------------------------------------------------------------------------

ox_core::define_id_newtype!(
    /// Type-safe identifier for node types in an ontology.
    NodeTypeId
);
ox_core::define_id_newtype!(
    /// Type-safe identifier for edge types in an ontology.
    EdgeTypeId
);
ox_core::define_id_newtype!(
    /// Type-safe identifier for property definitions.
    PropertyId
);
ox_core::define_id_newtype!(
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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct NodeTypeDef {
    /// Stable UUID for this node type.
    pub id: NodeTypeId,
    /// Canonical, language-neutral label used as the Neo4j node label and in
    /// query identifiers. The [`GraphLabel`] newtype enforces the
    /// `is_valid_graph_identifier` invariant at the type level — a
    /// `NodeTypeDef` cannot exist with a label that would fail Cypher
    /// emission.
    pub label: GraphLabel,
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

    // -------------------------------------------------------------------
    // Phase 5-B — semantic links back to the top-level collections
    //
    // Every field is optional on the wire so a v1 payload lands in an
    // empty v2 shape without a migration. The lists carry ids rather
    // than full `*Def` values so a node type can be rendered without
    // pulling every dependent type into memory — callers resolve the
    // ids through `OntologyIR::actions()` / `metrics()` / etc.
    // -------------------------------------------------------------------
    /// Interfaces this node type fulfils. Must reference
    /// `InterfaceDef`s declared on the enclosing `OntologyIR`; the
    /// validator rejects a node that claims to implement an interface
    /// without providing every required property / edge.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub implements: Vec<crate::interface::InterfaceId>,
    /// Actions (writes / mutations) that apply to this node type.
    /// Actions stay owned by the top-level `actions` collection; the
    /// node type just points at the ones it supports.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<crate::action::ActionId>,
    /// Metrics (KPIs / aggregates) scoped to this node type.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub metrics: Vec<crate::metric::MetricId>,
    /// Rules that govern this node type. Any rule whose `kind`
    /// targets this node is eligible; the planner cross-checks at
    /// query time.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<crate::action::RuleId>,
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

impl Default for NodeTypeDef {
    // `GraphLabel` has no `Default` — it refuses to pretend that an
    // empty or placeholder label is a valid one. But `NodeTypeDef` is
    // consumed primarily through struct-update syntax
    // (`NodeTypeDef { id, label, ..Default::default() }`), so the
    // `label` field slot must be populated with *something* even though
    // every real caller overwrites it. We construct a placeholder that
    // satisfies `GraphLabel`'s invariants but is deliberately
    // un-ontological; a missed override would surface loudly at the
    // ontology level (`validate()` rejects the sentinel).
    fn default() -> Self {
        Self {
            id: NodeTypeId::default(),
            #[allow(clippy::expect_used)]
            label: GraphLabel::new("__default_placeholder__")
                .expect("placeholder satisfies GraphLabel invariants"),
            display_name: LocalizedText::default(),
            description: LocalizedText::default(),
            properties: Vec::new(),
            constraints: Vec::new(),
            parent: None,
            governance: None,
            source_lineage: None,
            deprecated_at: None,
            replaced_by_id: None,
            implements: Vec::new(),
            actions: Vec::new(),
            metrics: Vec::new(),
            rules: Vec::new(),
        }
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
    /// "REVIEWED"). Used as the Neo4j relationship type. [`GraphLabel`]
    /// enforces the `is_valid_graph_identifier` invariant at the type
    /// level — an EdgeTypeDef cannot exist with a label that would
    /// fail Cypher emission.
    pub label: GraphLabel,
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
    // Same placeholder-label strategy as `NodeTypeDef::default` — the
    // slot must be filled even though every real caller uses
    // struct-update syntax to overwrite it, and a missed override is
    // caught at validate() via the placeholder sentinel check.
    fn default() -> Self {
        Self {
            id: EdgeTypeId::default(),
            #[allow(clippy::expect_used)]
            label: GraphLabel::new("__default_placeholder__")
                .expect("placeholder satisfies GraphLabel invariants"),
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
    /// "created_at"). Used as the Neo4j property key. [`PropertyKey`]
    /// enforces the `is_valid_graph_identifier` invariant at the type
    /// level — a PropertyDef cannot exist with a name that would fail
    /// Cypher emission.
    pub name: PropertyKey,
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
    /// Unit of measure for numeric properties — references a
    /// [`crate::code_system::CodedValue`] in a UCUM-compatible
    /// [`crate::code_system::CodeSystemDef`]. Replaces the
    /// previous free-form `unit: Option<String>` so the runtime
    /// can parse, convert, and dimension-check numeric values
    /// against the UCUM registry (no more `"kg" + "m"` silent
    /// nonsense). The UCUM code system is typically seeded as
    /// `CodeSystemKind::External { source_ref: "http://unitsofmeasure.org" }`
    /// but any [`crate::code_system::CodeSystemDef`] can supply
    /// custom units for domain-specific measures.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit_id: Option<crate::code_system::CodedValueId>,
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

    // -------------------------------------------------------------------
    // Phase 5-B — semantic annotations pointing at the top-level
    // collections.
    //
    // `glossary_term_id` links a technical property to the business
    // concept it realises — two `String` columns called `email` on
    // different nodes can now be distinguished through the glossary.
    // `aliases` + `business_context` give the LLM prompt richer
    // context than the technical name alone.
    // `derived_from` marks the property as computed by a `FunctionDef`;
    // the planner resolves the value at query time instead of reading
    // it from the mapped column.
    // -------------------------------------------------------------------
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub glossary_term_id: Option<crate::glossary::GlossaryTermId>,
    /// Localized alternate names used for synonym-aware UI search
    /// and for LLM prompts that normalise arbitrary user phrasing
    /// onto the property. Not required to be Cypher-safe — the
    /// property key is still the compile target.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<ox_core::i18n::LocalizedText>,
    /// Free-form business-context note (e.g. "extracted from legacy
    /// CRM; maps to CUST_STATUS column"). Shown in the property
    /// inspector and fed into the design-agent's context prompt.
    #[serde(default)]
    pub business_context: ox_core::i18n::LocalizedText,
    /// When set, the property is derived by evaluating the referenced
    /// function instead of being read from the physical source.
    /// Mutually exclusive with `source_column` at query time — the
    /// planner rejects a property that declares both.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub derived_from: Option<crate::function::FunctionId>,

    // -------------------------------------------------------------------
    // Phase Ω — terminology wiring. Each field points at a top-level
    // registry entry that the OntologyIR validator checks for
    // referential integrity. Runtime (LLM prompt context, validation
    // rules, admin UI) consumes them via the by_id accessors.
    // -------------------------------------------------------------------
    /// When set, values of this property must be codes from the
    /// referenced [`crate::value_set::ValueSetDef`]. The LLM prompt
    /// layer injects the expanded code list + display labels so
    /// natural-language filters like "status active" resolve onto
    /// the correct code. Runtime validation rejects writes whose
    /// value is not in the expanded set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value_set_id: Option<crate::value_set::ValueSetId>,

    /// When set, values of this property must conform to the
    /// referenced [`crate::notation_pattern::NotationPatternDef`]
    /// (e.g. `"SPRING_26_001"`). The runtime decomposes values
    /// into named components and exposes component-level queries
    /// ("all SPRING campaigns regardless of year") without
    /// repeated ad-hoc regex parsing in every caller.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notation_pattern_id: Option<crate::notation_pattern::NotationPatternId>,

    /// When set, numeric values of this property are classified
    /// against the referenced
    /// [`crate::value_range::ValueRangeSetDef`] — "blood pressure
    /// 90–120 is Normal", "cost 500k+ is Critical". The runtime
    /// exposes the classification label alongside the raw value;
    /// the LLM prompt layer injects band labels so natural-language
    /// queries like "high-cost orders" resolve deterministically.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value_range_set_id: Option<crate::value_range::ValueRangeSetId>,

    /// Π-1: Analytical role of this property. Drives NL2SQL
    /// prompt context so LLM-generated queries don't aggregate a
    /// dimension or group-by a measure. Optional because not
    /// every property has a well-defined role at authoring time;
    /// missing values fall back to LLM heuristics (with a quality
    /// gap diagnostic surfaced).
    ///
    /// Reference: dbt Semantic Layer, Cube.js measures/dimensions,
    /// Looker LookML `measure` / `dimension` declarations. The
    /// four-way split matches the industry consensus for NL2SQL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aggregation_role: Option<AggregationRole>,
}

/// Π-1: Analytical role of a property in a NL2SQL query — drives
/// whether the property appears in aggregation (`Measure`), group-by
/// (`Dimension`), carry-through projection (`Attribute`), or
/// identity (`Identifier`).
///
/// Industry reference:
/// - **dbt Semantic Layer** — `measure` / `dimension` / `entity`.
/// - **Cube.js** — `measures` / `dimensions`.
/// - **Looker LookML** — `measure` / `dimension`.
/// - **OMG SBVR** — noun-concept vs. verb-concept roles.
///
/// The `Identifier` variant maps to dbt's `entity` and is separate
/// from `Attribute` because LLMs treat identifiers very
/// differently (join keys vs. informational payload).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AggregationRole {
    /// Numeric property aggregated by metrics (SUM / AVG / MAX).
    /// Example: `order.total`, `product.quantity`.
    Measure,
    /// Property used as a group-by axis or filter. Example:
    /// `customer.region`, `order.status`.
    Dimension,
    /// Carry-through informational property — not aggregated, not
    /// typically grouped. Example: `customer.notes`.
    Attribute,
    /// Primary-key / entity-identifying property. Used for joins
    /// and row-level operations. Example: `customer.customer_id`.
    Identifier,
}

impl Default for PropertyDef {
    // Same sentinel strategy as the label defaults: the name slot
    // needs a populated `PropertyKey` because struct-update syntax
    // is the primary consumer, and a missed override shows up at
    // validate() time via the placeholder check.
    fn default() -> Self {
        Self {
            id: PropertyId::default(),
            #[allow(clippy::expect_used)]
            name: PropertyKey::new("__default_placeholder__")
                .expect("placeholder satisfies PropertyKey invariants"),
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
            unit_id: None,
            pii_kind: None,
            source_column: None,
            transform: None,
            deprecated_at: None,
            replaced_by_id: None,
            glossary_term_id: None,
            aliases: Vec::new(),
            business_context: LocalizedText::default(),
            derived_from: None,
            value_set_id: None,
            notation_pattern_id: None,
            value_range_set_id: None,
            aggregation_role: None,
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
    /// Full-text search index. `name` is a Cypher-quoted identifier
    /// (Neo4j FULLTEXT index names follow the same rules as labels),
    /// so the type-level guarantee on [`GraphLabel`] applies here too.
    FullText {
        id: String,
        name: GraphLabel,
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
