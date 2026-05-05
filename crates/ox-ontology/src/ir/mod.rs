mod cycles;
pub mod migration;
mod types;
mod validation;

#[cfg(test)]
mod tests;

pub use types::*;

use std::collections::HashMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use ox_core::graph_label::GraphLabel;

// ---------------------------------------------------------------------------
// OntologyInvariantError — typed invariant violations
// ---------------------------------------------------------------------------

/// Invariants that the ontology IR maintains at construction and after any
/// structural mutation. A violation means the caller produced inconsistent
/// data (duplicate ids, references to non-existent entities, etc.).
///
/// Raised by [`OntologyIR::try_new`], [`OntologyIR::rebuild_indices`], and
/// every mutation method that can introduce duplicates.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum OntologyInvariantError {
    #[error("duplicate node type id: {id}")]
    DuplicateNodeTypeId { id: NodeTypeId },

    #[error("duplicate node type label: {label}")]
    DuplicateNodeTypeLabel { label: GraphLabel },

    #[error("duplicate edge type id: {id}")]
    DuplicateEdgeTypeId { id: EdgeTypeId },

    #[error("duplicate property id: {id} (property ids must be unique across the ontology)")]
    DuplicatePropertyId { id: PropertyId },

    #[error("node type not found: {id}")]
    NodeTypeNotFound { id: NodeTypeId },

    #[error("edge type not found: {id}")]
    EdgeTypeNotFound { id: EdgeTypeId },

    #[error("index not found: {id}")]
    IndexNotFound { id: String },

    /// A semantic-superstructure or mapping collection ingested a
    /// duplicate id. Kept separate from the typed `Duplicate*Id`
    /// variants so we do not need a new variant per collection; the
    /// `kind` field names the collection (`"interface"`, `"rule"`,
    /// `"action"`, `"function"`, `"metric"`, `"enrichment"`,
    /// `"glossary_term"`, `"data_quality"`, `"object_mapping"`,
    /// `"link_mapping"`).
    #[error("duplicate {kind} id: {id}")]
    DuplicateCollectionId { kind: &'static str, id: String },

    /// A semantic-superstructure collection lookup / remove missed
    /// its target — typical on an edit-log UpdateX op whose
    /// expected_version matches current but the id has since been
    /// removed by a concurrent operator.
    #[error("{kind} not found: {id}")]
    CollectionEntryNotFound { kind: &'static str, id: String },

    /// A field on a definition references another id that either
    /// does not exist in the IR or points back at the owner itself.
    /// `kind` names the field path (e.g. `"glossary_term.replaced_by"`),
    /// `id` is the owning record, and `target` is the offending value.
    #[error("invalid reference {kind}: {id} → {target}")]
    InvalidReference {
        kind: &'static str,
        id: String,
        target: String,
    },
}

// ---------------------------------------------------------------------------
// OntologyIR — DB-agnostic ontology definition
//
// Describes the graph schema (node types, edge types, constraints, indexes)
// without any reference to Cypher, Gremlin, or GQL syntax.
//
// All entities carry stable UUIDs (`id` fields). Cross-references between
// entities use these IDs rather than labels/names so that renames do not
// break referential integrity.
//
// Compiles to:
//   Neo4j   → CREATE CONSTRAINT / CREATE INDEX statements
//   Neptune → Property graph schema (or schema-less with validation)
//   GQL     → CREATE NODE TYPE / CREATE EDGE TYPE (ISO 39075)
// ---------------------------------------------------------------------------

/// Tiered schema view. Each level trades fidelity for tokens; pick the
/// smallest one that gives the consuming LLM enough signal.
///
/// See [`OntologyIR::schema_view`] for per-tier semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchemaView {
    /// Node + edge label names only. Cheapest.
    Labels,
    /// Labels + per-node property names + 1-hop edge connectivity.
    Structural,
    /// Full property types, descriptions, edge cardinality (current
    /// `compact_schema` output). Most expensive — use only for the
    /// subset already known to be needed.
    Detailed,
}

/// Precomputed lookup indices for O(1) resolver access.
/// Rebuilt automatically on deserialization and after mutations.
///
/// Every collection gets its own id → index map so the validator
/// and planner read them in O(1) instead of scanning the owning
/// vector. The maps are populated in [`OntologyIR::rebuild_indices`]
/// and rebuilt on every structural mutation; a duplicate id in any
/// collection surfaces as
/// [`OntologyInvariantError::DuplicateCollectionId`] rather than
/// silently last-wins.
#[derive(Debug, Clone, Default)]
struct OntologyLookup {
    /// node id → index in node_types
    node_id_idx: HashMap<NodeTypeId, usize>,
    /// node label → index in node_types
    node_label_idx: HashMap<GraphLabel, usize>,
    /// edge id → index in edge_types
    edge_id_idx: HashMap<EdgeTypeId, usize>,
    /// property id → (node_types index, property index within that node)
    prop_id_loc: HashMap<PropertyId, (usize, usize)>,

    // --- Semantic superstructure + mapping indices ----------------------
    interface_id_idx: HashMap<crate::interface::InterfaceId, usize>,
    rule_id_idx: HashMap<crate::action::RuleId, usize>,
    action_id_idx: HashMap<crate::action::ActionId, usize>,
    function_id_idx: HashMap<crate::function::FunctionId, usize>,
    metric_id_idx: HashMap<crate::metric::MetricId, usize>,
    enrichment_id_idx: HashMap<crate::enrichment::EnrichmentId, usize>,
    glossary_term_id_idx: HashMap<crate::glossary::GlossaryTermId, usize>,
    data_quality_id_idx: HashMap<crate::data_quality::DataQualityId, usize>,
    object_mapping_id_idx: HashMap<crate::mapping::ObjectMappingId, usize>,
    link_mapping_id_idx: HashMap<crate::mapping::LinkMappingId, usize>,

    // Ω-1: terminology registry.
    code_system_id_idx: HashMap<crate::code_system::CodeSystemId, usize>,
    /// `CodedValueId → (code_systems index, codes index inside the
    /// system)` — O(1) lookup for downstream types that reference
    /// a code by its globally-unique id.
    coded_value_loc:
        HashMap<crate::code_system::CodedValueId, (usize, usize)>,
    // Ω-2: value sets.
    value_set_id_idx: HashMap<crate::value_set::ValueSetId, usize>,
    // Ω-4: notation patterns.
    notation_pattern_id_idx:
        HashMap<crate::notation_pattern::NotationPatternId, usize>,
    concept_map_id_idx: HashMap<crate::concept_map::ConceptMapId, usize>,
    value_range_set_id_idx: HashMap<crate::value_range::ValueRangeSetId, usize>,
    column_profile_id_idx: HashMap<crate::column_profile::ColumnProfileId, usize>,
    segment_id_idx: HashMap<crate::segment::SegmentId, usize>,
    /// `(source_id, table_name)` → row index. The natural key is
    /// composite so `add_table_inventory_entry` upserts cleanly and
    /// re-introspection stays idempotent.
    table_inventory_idx: HashMap<(crate::mapping::SourceId, String), usize>,
    /// Reverse index — glossary term → every NodeType that flags it
    /// as `concept_term_id`. Empty Vec when no NodeType realises the
    /// term (legal — the term stays as a vocabulary entry until an
    /// implementer is wired, or the term carries no `realisation` and
    /// is plain glossary content). Built from the forward
    /// `NodeTypeDef.concept_term_id` field, so adding / removing the
    /// pointer on a node automatically rebuilds the reverse view on
    /// the next `rebuild_indices()`.
    glossary_term_realised_by_node_types:
        HashMap<crate::glossary::GlossaryTermId, Vec<usize>>,
    /// Reverse index for edges; same shape as the node version.
    glossary_term_realised_by_edge_types:
        HashMap<crate::glossary::GlossaryTermId, Vec<usize>>,
}

/// Current on-wire schema version for `OntologyIR` JSONB. Bumped whenever
/// a backwards-incompatible shape change lands. Deserialisation rejects
/// a payload whose `schema_version` exceeds this number so we fail fast
/// instead of silently dropping new fields.
///
/// **Version history**
/// - `1` — original shape (node_types, edge_types, indexes).
/// - `2` — first-class collections added: `InterfaceDef`, `RuleDef`,
///   `ActionDef`, `FunctionDef`, `MetricDef`, `EnrichmentDef`,
///   `GlossaryTermDef`, `DataQualityDef`, and `ProvenanceDef`. Every
///   new collection defaults to empty, so a v1 payload deserialises
///   correctly into a v2 value without a migration pass — we still
///   bump the version so the server refuses to downgrade (a v2
///   ontology round-tripping through a v1 build would silently lose
///   these collections).
/// - `3` — adds the terminology registry collection
///   `code_systems: Vec<CodeSystemDef>` (with nested `CodedValue`
///   entries). Same defaults-to-empty guarantee — a v2 payload
///   round-trips through v3 unchanged.
/// - `4` — Phase Φ3 wiring: adds the
///   `column_profiles: Vec<ColumnProfileDef>` collection so the
///   data-distribution snapshot the introspection kernel produced
///   survives commit / hydrate cycles. Defaults-to-empty, so a v3
///   payload round-trips through v4 unchanged.
/// - `5` — Vocabulary unification: the standalone `concepts:
///   Vec<ConceptDef>` collection is folded into the glossary as
///   `GlossaryTermDef.realisation: Option<TermRealisation>`, and
///   `NodeTypeDef.concept_id` / `EdgeTypeDef.concept_id` are
///   replaced by `concept_term_id: Option<GlossaryTermId>` pointing
///   directly at the term that names the concept. The 1:1 anchor
///   that the old shape declared as an invariant is now structural:
///   the term *is* the concept. v4 payloads need an explicit
///   migration pass — concept rows must be rewritten as glossary
///   terms with `realisation` set.
pub const ONTOLOGY_IR_SCHEMA_VERSION: u32 = 5;

fn default_ontology_ir_schema_version() -> u32 {
    ONTOLOGY_IR_SCHEMA_VERSION
}

#[derive(Debug, Clone, Serialize, JsonSchema, utoipa::ToSchema)]
pub struct OntologyIR {
    /// On-wire struct shape version. See `ONTOLOGY_IR_SCHEMA_VERSION`
    /// for the live constant and version history.
    #[serde(default = "default_ontology_ir_schema_version")]
    pub schema_version: u32,
    /// Unique identifier for this ontology version.
    pub id: String,
    /// Canonical short identifier. Used as the URI fragment in OWL /
    /// SHACL exports, as the workspace-scoped uniqueness key in the
    /// store layer, and as the lineage-stable handle external systems
    /// reference. Single language by design — the locale-aware human
    /// label is `display_name`. Mirrors Foundry's `apiName` /
    /// Stardog's database identifier.
    pub name: String,
    /// Locale-aware human label. Empty by default; consumers fall
    /// back to `name` when no display string is set in the active
    /// chain. Routes the same `LocalizedText` shape every other
    /// user-facing field uses, so admin UI and LLM prompt context
    /// can render the workspace's `admin_locale_fallback` /
    /// `llm_locale_fallback` chains uniformly.
    #[serde(default)]
    pub display_name: ox_core::i18n::LocalizedText,
    /// Localized human-readable description of the ontology.
    #[serde(default)]
    pub description: ox_core::i18n::LocalizedText,
    /// Version metadata (number + temporal window + provenance).
    pub version: OntologyVersion,
    /// All node types in this ontology. Accessed externally via
    /// [`OntologyIR::node_types`]; structural mutations go through
    /// [`OntologyIR::add_node_type`], [`OntologyIR::remove_node_type`], etc.
    pub(crate) node_types: Vec<NodeTypeDef>,
    /// All edge types (relationships) in this ontology. See
    /// [`OntologyIR::edge_types`] / [`OntologyIR::add_edge_type`].
    #[serde(default)]
    pub(crate) edge_types: Vec<EdgeTypeDef>,
    /// Global indexes that span multiple types. See
    /// [`OntologyIR::indexes`] / [`OntologyIR::add_index`].
    #[serde(default)]
    pub(crate) indexes: Vec<IndexDef>,

    // -------------------------------------------------------------------
    // Semantic superstructure
    //
    // Every collection below is optional on the wire
    // (`#[serde(default)]`) so older payloads deserialise cleanly
    // and read paths go through accessor methods rather than these
    // fields directly.
    // -------------------------------------------------------------------
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) interfaces: Vec<crate::interface::InterfaceDef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) rules: Vec<crate::rule::RuleDef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) actions: Vec<crate::action::ActionDef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) functions: Vec<crate::function::FunctionDef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) metrics: Vec<crate::metric::MetricDef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) enrichments: Vec<crate::enrichment::EnrichmentDef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) glossary: Vec<crate::glossary::GlossaryTermDef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) data_quality: Vec<crate::data_quality::DataQualityDef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) provenance: Vec<crate::provenance::ProvenanceDef>,

    /// Object-level mappings. Binds a node type to a physical
    /// relation in a specific source. Multi-mapping per node type
    /// is allowed; the federation planner deduplicates using
    /// `precedence` + `primary_key_columns`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) object_mappings: Vec<crate::mapping::ObjectMappingDef>,
    /// Edge-level mappings. Binds an edge type to the relation(s)
    /// that supply edges — FK, bridge, computed, or federated
    /// across sources.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) link_mappings: Vec<crate::mapping::LinkMappingDef>,

    /// Terminology registry — named code systems with nested
    /// [`CodedValue`] entries. Phase Ω-1 foundation; value sets,
    /// concept maps, notation patterns, units of measure, and
    /// numeric range bands all layer on top of this.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) code_systems: Vec<crate::code_system::CodeSystemDef>,

    /// Ω-2: value sets — bounded subsets of one or more code
    /// systems. A property's `value_set_id` points here.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) value_sets: Vec<crate::value_set::ValueSetDef>,

    /// Ω-4: notation patterns — structured identifier formats.
    /// A property's `notation_pattern_id` points here.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) notation_patterns: Vec<crate::notation_pattern::NotationPatternDef>,

    /// Ω-5: concept maps — declarative code↔code translations
    /// between two [`crate::code_system::CodeSystemDef`]s.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) concept_maps: Vec<crate::concept_map::ConceptMapDef>,

    /// Ω-7: numeric interpretive band sets — "normal / elevated /
    /// high" style value classifications over a numeric property.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) value_range_sets: Vec<crate::value_range::ValueRangeSetDef>,

    /// Per-column distribution snapshots, keyed by
    /// `(source_id, relation, column)`. Ingested from
    /// `SourceProfile` via [`OntologyIR::ingest_source_profile`] so
    /// re-running value-set / notation-pattern inference doesn't
    /// require a source rescan.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) column_profiles: Vec<crate::column_profile::ColumnProfileDef>,

    /// Named, reusable membership predicates over a single NodeType
    /// (`SegmentDef` — "Active Customer", "Lapsed Subscriber",
    /// "VIP"). First-class so segment references from
    /// `PropertyBinding`, `RuleDef`, and the glossary realisation
    /// surface resolve through `OntologyLookup` without
    /// round-tripping a side artifact.
    /// Validated by `validate()`: `target_node_type_id` must
    /// resolve, every `referenced_properties()` entry must exist
    /// on the target, and SegmentId values are unique.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) segments: Vec<crate::segment::SegmentDef>,

    /// Source-table inventory: one row per `(source_id, table_name)`
    /// the project has touched, carrying import status, schema
    /// fingerprint, and the contributed NodeType / EdgeType ids.
    /// Answers "which tables did this project bring in?" / "where
    /// did this NodeType come from?" / "what's available but not
    /// yet imported?" without re-introspecting.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) table_inventory: Vec<crate::table_inventory::TableInventoryEntry>,

    /// Precomputed lookup indices — not serialized, rebuilt on deserialize.
    #[serde(skip)]
    #[schemars(skip)]
    lookup: OntologyLookup,
}

/// Custom Deserialize that runs the JSONB migration pipeline,
/// auto-builds lookup indices after loading, and rejects payloads
/// from a future struct shape.
///
/// The deserialiser materialises the incoming payload as a
/// `serde_json::Value`, hands it to [`migration::migrate_to_current`]
/// to walk the version chain, and only then decodes into the typed
/// `Wire` shape. This means a JSONB row written by a v3 server
/// loads cleanly on a v5 server: the v4→v5 migration (concept fold-in)
/// runs first, the resulting v5 shape decodes into `Wire`, and the
/// post-image is structurally identical to a freshly-written row.
impl<'de> Deserialize<'de> for OntologyIR {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Materialise as Value first so the migration pipeline can
        // walk the JSON shape. This adds one round-trip allocation
        // — small price for forward-compatible reads of every
        // historical schema version.
        let raw = serde_json::Value::deserialize(deserializer)?;
        let migrated = migration::migrate_to_current(raw)
            .map_err(|e| serde::de::Error::custom(e.to_string()))?;

        #[derive(Deserialize)]
        struct Wire {
            #[serde(default = "default_ontology_ir_schema_version")]
            schema_version: u32,
            id: String,
            name: String,
            #[serde(default)]
            description: ox_core::i18n::LocalizedText,
            version: OntologyVersion,
            node_types: Vec<NodeTypeDef>,
            #[serde(default)]
            edge_types: Vec<EdgeTypeDef>,
            #[serde(default)]
            indexes: Vec<IndexDef>,
            #[serde(default)]
            interfaces: Vec<crate::interface::InterfaceDef>,
            #[serde(default)]
            rules: Vec<crate::rule::RuleDef>,
            #[serde(default)]
            actions: Vec<crate::action::ActionDef>,
            #[serde(default)]
            functions: Vec<crate::function::FunctionDef>,
            #[serde(default)]
            metrics: Vec<crate::metric::MetricDef>,
            #[serde(default)]
            enrichments: Vec<crate::enrichment::EnrichmentDef>,
            #[serde(default)]
            glossary: Vec<crate::glossary::GlossaryTermDef>,
            #[serde(default)]
            data_quality: Vec<crate::data_quality::DataQualityDef>,
            #[serde(default)]
            provenance: Vec<crate::provenance::ProvenanceDef>,
            #[serde(default)]
            object_mappings: Vec<crate::mapping::ObjectMappingDef>,
            #[serde(default)]
            link_mappings: Vec<crate::mapping::LinkMappingDef>,
            #[serde(default)]
            code_systems: Vec<crate::code_system::CodeSystemDef>,
            #[serde(default)]
            value_sets: Vec<crate::value_set::ValueSetDef>,
            #[serde(default)]
            notation_patterns: Vec<crate::notation_pattern::NotationPatternDef>,
            #[serde(default)]
            concept_maps: Vec<crate::concept_map::ConceptMapDef>,
            #[serde(default)]
            value_range_sets: Vec<crate::value_range::ValueRangeSetDef>,
            #[serde(default)]
            column_profiles: Vec<crate::column_profile::ColumnProfileDef>,
        }

        let w: Wire = serde_json::from_value(migrated)
            .map_err(serde::de::Error::custom)?;
        // The migration pipeline already rejected any future
        // version; the post-image carries a current-or-older tag.
        // The redundant guard below stays as defence in depth — a
        // bug in the chain that produced a payload tagged "future"
        // surfaces here rather than corrupting downstream code.
        if w.schema_version > ONTOLOGY_IR_SCHEMA_VERSION {
            return Err(serde::de::Error::custom(format!(
                "OntologyIR schema_version {} is newer than this build supports (max {}). \
                 Upgrade the server or export/import through a compatible version.",
                w.schema_version, ONTOLOGY_IR_SCHEMA_VERSION,
            )));
        }
        let mut ont = OntologyIR::try_new(
            w.id,
            w.name,
            w.description,
            w.version,
            w.node_types,
            w.edge_types,
            w.indexes,
        )
        .map_err(serde::de::Error::custom)?;
        // Populate the superstructure after base construction, then
        // rebuild the lookup once more so the superstructure
        // indices cover the new collections. `try_new` already
        // built the node/edge/property indices; the second pass is
        // idempotent over those and populates the new id→index maps.
        ont.interfaces = w.interfaces;
        ont.rules = w.rules;
        ont.actions = w.actions;
        ont.functions = w.functions;
        ont.metrics = w.metrics;
        ont.enrichments = w.enrichments;
        ont.glossary = w.glossary;
        ont.data_quality = w.data_quality;
        ont.provenance = w.provenance;
        ont.object_mappings = w.object_mappings;
        ont.link_mappings = w.link_mappings;
        ont.code_systems = w.code_systems;
        ont.value_sets = w.value_sets;
        ont.notation_patterns = w.notation_patterns;
        ont.concept_maps = w.concept_maps;
        ont.value_range_sets = w.value_range_sets;
        ont.column_profiles = w.column_profiles;
        ont.rebuild_indices().map_err(serde::de::Error::custom)?;
        Ok(ont)
    }
}

// ---------------------------------------------------------------------------
// Construction + Index management + Resolver methods (O(1) via HashMap)
// ---------------------------------------------------------------------------

impl OntologyIR {
    /// Construct a new OntologyIR with prebuilt lookup indices.
    ///
    /// This is the ergonomic constructor used when the caller has just built
    /// the vectors with known-unique ids/labels (e.g. test fixtures, transform
    /// helpers). It delegates to [`OntologyIR::try_new`] and panics on invariant
    /// violations — such a panic indicates a programming bug in the caller.
    ///
    /// For deserialization or any input whose uniqueness cannot be guaranteed
    /// up-front, call [`OntologyIR::try_new`] directly and handle the
    /// [`OntologyInvariantError`] explicitly.
    #[allow(clippy::expect_used)]
    pub fn new(
        id: String,
        name: String,
        description: ox_core::i18n::LocalizedText,
        version: impl Into<OntologyVersion>,
        node_types: Vec<NodeTypeDef>,
        edge_types: Vec<EdgeTypeDef>,
        indexes: Vec<IndexDef>,
    ) -> Self {
        Self::try_new(
            id,
            name,
            description,
            version,
            node_types,
            edge_types,
            indexes,
        )
        .expect(
            "OntologyIR::new called with duplicate ids/labels; \
                 caller must ensure uniqueness or use OntologyIR::try_new instead",
        )
    }

    /// Render the ontology's locale-aware label, falling back to
    /// `name` when no `display_name` is set in the supplied chain.
    /// Surfaces (admin UI, LLM context) thread the workspace chain
    /// through `useLocaleChain()` / `LocaleChain::admin()` so the
    /// same IR renders the workspace's preferred language without
    /// embedding locale into the canonical identifier.
    pub fn display_label(&self, chain: &[ox_core::i18n::LanguageTag]) -> String {
        let resolved = self.display_name.resolve(chain);
        if resolved.is_empty() {
            self.name.clone()
        } else {
            resolved.to_string()
        }
    }

    /// Fallible constructor. Returns [`OntologyInvariantError`] if the input
    /// vectors contain duplicate node/edge/property ids or duplicate node
    /// labels.
    ///
    /// Use this whenever input cannot be statically guaranteed unique —
    /// deserialization, merging ontologies, LLM-generated data, etc.
    pub fn try_new(
        id: String,
        name: String,
        description: ox_core::i18n::LocalizedText,
        version: impl Into<OntologyVersion>,
        node_types: Vec<NodeTypeDef>,
        edge_types: Vec<EdgeTypeDef>,
        indexes: Vec<IndexDef>,
    ) -> Result<Self, OntologyInvariantError> {
        let mut ont = Self {
            schema_version: ONTOLOGY_IR_SCHEMA_VERSION,
            id,
            name,
            display_name: ox_core::i18n::LocalizedText::default(),
            description,
            version: version.into(),
            node_types,
            edge_types,
            indexes,
            // Superstructure starts empty — populate via the typed
            // setters (`add_rule`, `add_action`, ...).
            interfaces: Vec::new(),
            rules: Vec::new(),
            actions: Vec::new(),
            functions: Vec::new(),
            metrics: Vec::new(),
            enrichments: Vec::new(),
            glossary: Vec::new(),
            data_quality: Vec::new(),
            provenance: Vec::new(),
            object_mappings: Vec::new(),
            link_mappings: Vec::new(),
            code_systems: Vec::new(),
            value_sets: Vec::new(),
            notation_patterns: Vec::new(),
            concept_maps: Vec::new(),
            value_range_sets: Vec::new(),
            column_profiles: Vec::new(),
            segments: Vec::new(),
            table_inventory: Vec::new(),
            lookup: OntologyLookup::default(),
        };
        ont.rebuild_indices()?;
        Ok(ont)
    }

    /// Project this IR to its state at the given instant.
    ///
    /// Filters every collection whose entries carry a temporal
    /// window — `RuleDef`, `ObjectMappingDef`, `PropertyBinding`
    /// — keeping only entries whose `[valid_from, valid_to)` window
    /// covers `at`. Untemporal collections (node types, edge types,
    /// glossary terms, code systems, etc.) pass through unchanged.
    ///
    /// The returned IR's lookup indices are rebuilt so every `*_by_id`
    /// accessor reflects the filtered slice. Returns an error iff the
    /// rebuild surfaces an invariant violation (which can only happen
    /// if the source IR was malformed before filtering).
    pub fn as_of(
        &self,
        at: chrono::DateTime<chrono::Utc>,
    ) -> Result<Self, OntologyInvariantError> {
        let mut out = self.clone();

        // Object mappings: drop any whose own window doesn't cover
        // `at`. ObjectMappingDef carries its window directly.
        out.object_mappings.retain(|om| {
            let starts_ok = om.valid_from.map(|f| at >= f).unwrap_or(true);
            let ends_ok = om.valid_to.map(|t| at < t).unwrap_or(true);
            starts_ok && ends_ok
        });

        // Rules: same window contract.
        out.rules.retain(|r| r.covers(at));

        // Property bindings: per-property filter — drop bindings
        // whose own window doesn't cover `at`, leaving the host
        // property in place.
        for node in &mut out.node_types {
            for prop in &mut node.properties {
                prop.bindings.retain(|b| b.covers(at));
            }
        }
        for edge in &mut out.edge_types {
            for prop in &mut edge.properties {
                prop.bindings.retain(|b| b.covers(at));
            }
        }

        out.rebuild_indices()?;
        Ok(out)
    }

    /// Construct a new OntologyIR, validate it, and return the validated instance.
    ///
    /// Structural invariant violations (duplicate ids/labels) surface as
    /// `ontology.invariant.*` codes; semantic validation issues use the
    /// `ontology.validate.*` family from
    /// [`OntologyIR::validate`]. Either way the `Err` carries
    /// structured [`DiagnosticMessage`]s the FE renders through its
    /// i18n catalogue.
    pub fn new_validated(
        id: String,
        name: String,
        description: ox_core::i18n::LocalizedText,
        version: impl Into<OntologyVersion>,
        node_types: Vec<NodeTypeDef>,
        edge_types: Vec<EdgeTypeDef>,
        indexes: Vec<IndexDef>,
    ) -> Result<Self, Vec<ox_core::DiagnosticMessage>> {
        let ont = Self::try_new(
            id,
            name,
            description,
            version,
            node_types,
            edge_types,
            indexes,
        )
        .map_err(|e| {
            vec![
                ox_core::diagnostic::diag("ontology.invariant.try_new")
                    .with("error", e.to_string())
                    .message(e.to_string()),
            ]
        })?;
        let errors = ont.validate();
        if errors.is_empty() {
            Ok(ont)
        } else {
            Err(errors)
        }
    }

    /// Rebuild all lookup indices from current data.
    ///
    /// Must be called after any structural mutation (add/remove/reorder
    /// nodes/edges/properties). Returns an error if duplicate ids or labels
    /// are detected — a programming mistake in the caller, not a user input
    /// error.
    ///
    /// Structural mutation methods on this type (`add_node_type`,
    /// `remove_edge_type`, `with_batch`, etc.) call this automatically, so
    /// most callers never need to invoke it directly.
    pub fn rebuild_indices(&mut self) -> Result<(), OntologyInvariantError> {
        let mut lookup = OntologyLookup::default();
        for (i, node) in self.node_types.iter().enumerate() {
            if lookup.node_id_idx.insert(node.id.clone(), i).is_some() {
                return Err(OntologyInvariantError::DuplicateNodeTypeId {
                    id: node.id.clone(),
                });
            }
            if lookup
                .node_label_idx
                .insert(node.label.clone(), i)
                .is_some()
            {
                return Err(OntologyInvariantError::DuplicateNodeTypeLabel {
                    label: node.label.clone(),
                });
            }
            for (j, prop) in node.properties.iter().enumerate() {
                if lookup.prop_id_loc.insert(prop.id.clone(), (i, j)).is_some() {
                    return Err(OntologyInvariantError::DuplicatePropertyId {
                        id: prop.id.clone(),
                    });
                }
            }
        }
        for (i, edge) in self.edge_types.iter().enumerate() {
            if lookup.edge_id_idx.insert(edge.id.clone(), i).is_some() {
                return Err(OntologyInvariantError::DuplicateEdgeTypeId {
                    id: edge.id.clone(),
                });
            }
        }

        // --- Semantic superstructure + mapping indices ------------------
        //
        // The loop below is mechanically repetitive because each
        // collection has its own typed id; the generic
        // `DuplicateCollectionId` carries a `kind` tag so the error
        // still points at the offending collection. A macro to
        // collapse the ten blocks would save lines at the cost of
        // reviewability — the current shape stays linear so a
        // reader sees exactly which collection each check applies
        // to.
        macro_rules! index_collection {
            ($field:ident, $idx:ident, $kind:literal) => {
                for (i, item) in self.$field.iter().enumerate() {
                    if lookup.$idx.insert(item.id.clone(), i).is_some() {
                        return Err(OntologyInvariantError::DuplicateCollectionId {
                            kind: $kind,
                            id: item.id.to_string(),
                        });
                    }
                }
            };
        }
        index_collection!(interfaces, interface_id_idx, "interface");
        index_collection!(rules, rule_id_idx, "rule");
        index_collection!(actions, action_id_idx, "action");
        index_collection!(functions, function_id_idx, "function");
        index_collection!(metrics, metric_id_idx, "metric");
        index_collection!(enrichments, enrichment_id_idx, "enrichment");
        index_collection!(glossary, glossary_term_id_idx, "glossary_term");
        index_collection!(data_quality, data_quality_id_idx, "data_quality");
        index_collection!(object_mappings, object_mapping_id_idx, "object_mapping");
        index_collection!(link_mappings, link_mapping_id_idx, "link_mapping");

        // Ω-1 terminology registry: top-level code_systems id index
        // plus nested coded_value id index. CodedValueIds are
        // globally unique inside an OntologyIR (they cross systems
        // for hierarchy + replacement references), so duplicates
        // here are a hard error regardless of which system they
        // appear in.
        for (i, system) in self.code_systems.iter().enumerate() {
            if lookup.code_system_id_idx.insert(system.id.clone(), i).is_some() {
                return Err(OntologyInvariantError::DuplicateCollectionId {
                    kind: "code_system",
                    id: system.id.to_string(),
                });
            }
            for (j, cv) in system.codes.iter().enumerate() {
                if lookup.coded_value_loc.insert(cv.id.clone(), (i, j)).is_some() {
                    return Err(OntologyInvariantError::DuplicateCollectionId {
                        kind: "coded_value",
                        id: cv.id.to_string(),
                    });
                }
            }
        }
        index_collection!(value_sets, value_set_id_idx, "value_set");
        index_collection!(notation_patterns, notation_pattern_id_idx, "notation_pattern");
        index_collection!(concept_maps, concept_map_id_idx, "concept_map");
        index_collection!(value_range_sets, value_range_set_id_idx, "value_range_set");
        index_collection!(column_profiles, column_profile_id_idx, "column_profile");
        index_collection!(segments, segment_id_idx, "segment");

        // Duplicate `(source_id, table_name)` rows violate the
        // composite key — `add_table_inventory_entry` upserts on
        // it, so duplicates at deserialise time mean a corrupt
        // payload.
        for (i, entry) in self.table_inventory.iter().enumerate() {
            let key = (entry.source_id.clone(), entry.table_name.clone());
            if lookup
                .table_inventory_idx
                .insert(key, i)
                .is_some()
            {
                return Err(OntologyInvariantError::DuplicateCollectionId {
                    kind: "table_inventory",
                    id: format!("{}/{}", entry.source_id, entry.table_name),
                });
            }
        }

        // Reverse index: glossary term → every implementing
        // NodeType / EdgeType. Built from the forward
        // `concept_term_id` pointer on each type. A term with no
        // implementer carries an empty Vec (or simply doesn't appear
        // in the map — both shapes are distinguishable through
        // `lookup.glossary_term_id_idx.contains_key`).
        for (i, node) in self.node_types.iter().enumerate() {
            if let Some(term_id) = &node.concept_term_id {
                lookup
                    .glossary_term_realised_by_node_types
                    .entry(term_id.clone())
                    .or_default()
                    .push(i);
            }
        }
        for (i, edge) in self.edge_types.iter().enumerate() {
            if let Some(term_id) = &edge.concept_term_id {
                lookup
                    .glossary_term_realised_by_edge_types
                    .entry(term_id.clone())
                    .or_default()
                    .push(i);
            }
        }

        self.lookup = lookup;
        Ok(())
    }

    /// Consume self, rebuild indices, return self. Useful for chaining after
    /// ad-hoc mutation patterns; fails with [`OntologyInvariantError`] on
    /// duplicate detection.
    pub fn with_indices(mut self) -> Result<Self, OntologyInvariantError> {
        self.rebuild_indices()?;
        Ok(self)
    }

    // -----------------------------------------------------------------------
    // Structural mutation API — every method rebuilds the lookup internally
    // and surfaces invariant violations via [`OntologyInvariantError`].
    // -----------------------------------------------------------------------

    /// Add a node type. Fails if a node with the same id or label already
    /// exists. On success, returns an immutable reference to the inserted
    /// entry.
    pub fn add_node_type(
        &mut self,
        node: NodeTypeDef,
    ) -> Result<&NodeTypeDef, OntologyInvariantError> {
        if self.lookup.node_id_idx.contains_key(&node.id) {
            return Err(OntologyInvariantError::DuplicateNodeTypeId { id: node.id });
        }
        if self.lookup.node_label_idx.contains_key(&node.label) {
            return Err(OntologyInvariantError::DuplicateNodeTypeLabel { label: node.label });
        }
        self.node_types.push(node);
        self.rebuild_indices()?;
        self.node_types
            .last()
            .ok_or_else(|| OntologyInvariantError::NodeTypeNotFound {
                id: NodeTypeId::new(""),
            })
    }

    /// Remove a node type by id and return the removed entry. Fails with
    /// [`OntologyInvariantError::NodeTypeNotFound`] if no such id exists.
    ///
    /// Edge types and indexes that reference the removed node are NOT
    /// cascaded — the caller is responsible for cleaning up references,
    /// typically via `ontology_command::OntologyCommand::DeleteNode`.
    pub fn remove_node_type(
        &mut self,
        id: &NodeTypeId,
    ) -> Result<NodeTypeDef, OntologyInvariantError> {
        let idx = *self
            .lookup
            .node_id_idx
            .get(id)
            .ok_or_else(|| OntologyInvariantError::NodeTypeNotFound { id: id.clone() })?;
        let removed = self.node_types.remove(idx);
        self.rebuild_indices()?;
        Ok(removed)
    }

    /// Apply a closure to a node type in place.
    ///
    /// The closure receives a mutable reference to the entry; on return, the
    /// lookup is rebuilt and invariant violations surface as
    /// [`OntologyInvariantError`] (e.g. if the closure renamed the node to a
    /// duplicate label).
    pub fn update_node_type<F>(
        &mut self,
        id: &NodeTypeId,
        f: F,
    ) -> Result<(), OntologyInvariantError>
    where
        F: FnOnce(&mut NodeTypeDef),
    {
        let idx = *self
            .lookup
            .node_id_idx
            .get(id)
            .ok_or_else(|| OntologyInvariantError::NodeTypeNotFound { id: id.clone() })?;
        f(&mut self.node_types[idx]);
        self.rebuild_indices()
    }

    /// Add an edge type. Fails on duplicate id.
    pub fn add_edge_type(
        &mut self,
        edge: EdgeTypeDef,
    ) -> Result<&EdgeTypeDef, OntologyInvariantError> {
        if self.lookup.edge_id_idx.contains_key(&edge.id) {
            return Err(OntologyInvariantError::DuplicateEdgeTypeId { id: edge.id });
        }
        self.edge_types.push(edge);
        self.rebuild_indices()?;
        self.edge_types
            .last()
            .ok_or_else(|| OntologyInvariantError::EdgeTypeNotFound {
                id: EdgeTypeId::new(""),
            })
    }

    /// Remove an edge type by id and return the removed entry.
    pub fn remove_edge_type(
        &mut self,
        id: &EdgeTypeId,
    ) -> Result<EdgeTypeDef, OntologyInvariantError> {
        let idx = *self
            .lookup
            .edge_id_idx
            .get(id)
            .ok_or_else(|| OntologyInvariantError::EdgeTypeNotFound { id: id.clone() })?;
        let removed = self.edge_types.remove(idx);
        self.rebuild_indices()?;
        Ok(removed)
    }

    /// Apply a closure to an edge type in place.
    pub fn update_edge_type<F>(
        &mut self,
        id: &EdgeTypeId,
        f: F,
    ) -> Result<(), OntologyInvariantError>
    where
        F: FnOnce(&mut EdgeTypeDef),
    {
        let idx = *self
            .lookup
            .edge_id_idx
            .get(id)
            .ok_or_else(|| OntologyInvariantError::EdgeTypeNotFound { id: id.clone() })?;
        f(&mut self.edge_types[idx]);
        self.rebuild_indices()
    }

    /// Add an index definition. No uniqueness check on ids (indexes carry
    /// caller-supplied ids that are not structural identifiers).
    pub fn add_index(&mut self, index: IndexDef) -> Result<(), OntologyInvariantError> {
        self.indexes.push(index);
        self.rebuild_indices()
    }

    /// Remove an index whose `id` matches `index_id`.
    pub fn remove_index(&mut self, index_id: &str) -> Result<IndexDef, OntologyInvariantError> {
        let pos = self.indexes.iter().position(|idx| match idx {
            IndexDef::Single { id, .. }
            | IndexDef::Composite { id, .. }
            | IndexDef::FullText { id, .. }
            | IndexDef::Vector { id, .. } => id == index_id,
        });
        let pos = pos.ok_or_else(|| OntologyInvariantError::IndexNotFound {
            id: index_id.to_string(),
        })?;
        let removed = self.indexes.remove(pos);
        self.rebuild_indices()?;
        Ok(removed)
    }

    /// Execute a batch of structural mutations with a single rebuild at the
    /// end.
    ///
    /// Use when a single logical change touches several entries (e.g.
    /// renaming a node and rewriting all edges that reference it). The
    /// closure receives direct mutable access to the internal vectors; on
    /// return, the lookup is rebuilt once and invariant violations surface
    /// as [`OntologyInvariantError`].
    pub fn with_batch<F, R>(&mut self, f: F) -> Result<R, OntologyInvariantError>
    where
        F: FnOnce(&mut Vec<NodeTypeDef>, &mut Vec<EdgeTypeDef>, &mut Vec<IndexDef>) -> R,
    {
        let r = f(
            &mut self.node_types,
            &mut self.edge_types,
            &mut self.indexes,
        );
        self.rebuild_indices()?;
        Ok(r)
    }

    // -----------------------------------------------------------------------
    // Read accessors — prefer these over direct field access.
    // -----------------------------------------------------------------------

    /// All node types in declaration order.
    pub fn node_types(&self) -> &[NodeTypeDef] {
        &self.node_types
    }

    /// All edge types in declaration order.
    pub fn edge_types(&self) -> &[EdgeTypeDef] {
        &self.edge_types
    }

    /// All indexes in declaration order.
    pub fn indexes(&self) -> &[IndexDef] {
        &self.indexes
    }

    /// Mutable slice of node types. Intended for field-level mutation that
    /// does **not** change `id` or `label` (e.g. description/property edits).
    /// If you do change `id`/`label`, call [`OntologyIR::rebuild_indices`]
    /// afterwards to refresh the lookup tables; for structural add/remove,
    /// use [`OntologyIR::add_node_type`] / [`OntologyIR::remove_node_type`].
    pub fn node_types_mut(&mut self) -> &mut [NodeTypeDef] {
        &mut self.node_types
    }

    /// Mutable slice of edge types. Same contract as
    /// [`OntologyIR::node_types_mut`].
    pub fn edge_types_mut(&mut self) -> &mut [EdgeTypeDef] {
        &mut self.edge_types
    }

    /// Mutable slice of index definitions. Same contract as
    /// [`OntologyIR::node_types_mut`].
    pub fn indexes_mut(&mut self) -> &mut [IndexDef] {
        &mut self.indexes
    }

    // -------------------------------------------------------------------
    // Superstructure accessors. Read-only — mutation goes through
    // the typed setters so schema-version bumps stay observable.
    // -------------------------------------------------------------------

    pub fn interfaces(&self) -> &[crate::interface::InterfaceDef] {
        &self.interfaces
    }

    pub fn rules(&self) -> &[crate::rule::RuleDef] {
        &self.rules
    }

    pub fn actions(&self) -> &[crate::action::ActionDef] {
        &self.actions
    }

    pub fn functions(&self) -> &[crate::function::FunctionDef] {
        &self.functions
    }

    pub fn metrics(&self) -> &[crate::metric::MetricDef] {
        &self.metrics
    }

    pub fn enrichments(&self) -> &[crate::enrichment::EnrichmentDef] {
        &self.enrichments
    }

    pub fn glossary(&self) -> &[crate::glossary::GlossaryTermDef] {
        &self.glossary
    }

    pub fn data_quality(&self) -> &[crate::data_quality::DataQualityDef] {
        &self.data_quality
    }

    pub fn provenance(&self) -> &[crate::provenance::ProvenanceDef] {
        &self.provenance
    }

    pub fn object_mappings(&self) -> &[crate::mapping::ObjectMappingDef] {
        &self.object_mappings
    }

    pub fn code_systems(&self) -> &[crate::code_system::CodeSystemDef] {
        &self.code_systems
    }

    pub fn value_sets(&self) -> &[crate::value_set::ValueSetDef] {
        &self.value_sets
    }

    pub fn notation_patterns(&self) -> &[crate::notation_pattern::NotationPatternDef] {
        &self.notation_patterns
    }

    pub fn concept_maps(&self) -> &[crate::concept_map::ConceptMapDef] {
        &self.concept_maps
    }

    pub fn value_range_sets(&self) -> &[crate::value_range::ValueRangeSetDef] {
        &self.value_range_sets
    }

    pub fn link_mappings(&self) -> &[crate::mapping::LinkMappingDef] {
        &self.link_mappings
    }

    /// Every per-column distribution snapshot the IR carries.
    /// Ingested from a [`SourceProfile`] via
    /// [`OntologyIR::ingest_source_profile`].
    pub fn column_profiles(&self) -> &[crate::column_profile::ColumnProfileDef] {
        &self.column_profiles
    }

    /// Every named NodeType-membership predicate (`SegmentDef`).
    /// Glossary realisation, rule anchors, and the query-time
    /// disambiguator all resolve through this collection.
    pub fn segments(&self) -> &[crate::segment::SegmentDef] {
        &self.segments
    }

    /// Every glossary term whose `realisation` field declares an
    /// executable concept spec (Segment / Function / CrossEntity).
    /// Terms without a realisation are pure vocabulary entries and
    /// stay out of this view; consumers that want the full glossary
    /// walk [`OntologyIR::glossary`] directly.
    pub fn concept_terms(
        &self,
    ) -> impl Iterator<Item = &crate::glossary::GlossaryTermDef> {
        self.glossary
            .iter()
            .filter(|term| term.realisation.is_some())
    }

    /// Every source-table inventory row the IR carries.
    pub fn table_inventory(&self) -> &[crate::table_inventory::TableInventoryEntry] {
        &self.table_inventory
    }

    /// O(1) lookup of an inventory row by its natural key. The FE's
    /// Source Inspector reaches for this on every table-detail
    /// render; the index makes the call free.
    pub fn find_table_inventory_entry(
        &self,
        source_id: &crate::mapping::SourceId,
        table_name: &str,
    ) -> Option<&crate::table_inventory::TableInventoryEntry> {
        let key = (source_id.clone(), table_name.to_string());
        self.lookup
            .table_inventory_idx
            .get(&key)
            .map(|&i| &self.table_inventory[i])
    }

    /// O(1) lookup: every NodeType that flags itself as realising
    /// the named glossary term. Reads the reverse index built in
    /// [`rebuild_indices`]. Empty slice when the term has no
    /// implementing nodes (legitimate transient state — a term can
    /// declare a `realisation` before its first implementer lands).
    pub fn node_types_realising_glossary_term(
        &self,
        term_id: &crate::glossary::GlossaryTermId,
    ) -> Vec<&NodeTypeDef> {
        self.lookup
            .glossary_term_realised_by_node_types
            .get(term_id)
            .map(|indices| indices.iter().map(|&i| &self.node_types[i]).collect())
            .unwrap_or_default()
    }

    /// Edge counterpart of [`node_types_realising_glossary_term`].
    pub fn edge_types_realising_glossary_term(
        &self,
        term_id: &crate::glossary::GlossaryTermId,
    ) -> Vec<&EdgeTypeDef> {
        self.lookup
            .glossary_term_realised_by_edge_types
            .get(term_id)
            .map(|indices| indices.iter().map(|&i| &self.edge_types[i]).collect())
            .unwrap_or_default()
    }

    /// Append an interface definition. Fails with
    /// [`OntologyInvariantError::DuplicateCollectionId`] when the id
    /// is already in use.
    ///
    /// The sibling add-methods below follow the same contract —
    /// push, then [`OntologyIR::rebuild_indices`] to keep the
    /// lookup tables authoritative. Callers that stage many
    /// additions should prefer [`OntologyIR::with_batch`] (not yet
    /// implemented for these collections; drop-in equivalent:
    /// push into the field directly then call `rebuild_indices`
    /// once at the end).
    pub fn add_interface(
        &mut self,
        def: crate::interface::InterfaceDef,
    ) -> Result<(), OntologyInvariantError> {
        if self.lookup.interface_id_idx.contains_key(&def.id) {
            return Err(OntologyInvariantError::DuplicateCollectionId {
                kind: "interface",
                id: def.id.to_string(),
            });
        }
        self.interfaces.push(def);
        self.rebuild_indices()
    }

    pub fn add_rule(
        &mut self,
        def: crate::rule::RuleDef,
    ) -> Result<(), OntologyInvariantError> {
        if self.lookup.rule_id_idx.contains_key(&def.id) {
            return Err(OntologyInvariantError::DuplicateCollectionId {
                kind: "rule",
                id: def.id.to_string(),
            });
        }
        self.rules.push(def);
        self.rebuild_indices()
    }

    pub fn add_action(
        &mut self,
        def: crate::action::ActionDef,
    ) -> Result<(), OntologyInvariantError> {
        if self.lookup.action_id_idx.contains_key(&def.id) {
            return Err(OntologyInvariantError::DuplicateCollectionId {
                kind: "action",
                id: def.id.to_string(),
            });
        }
        self.actions.push(def);
        self.rebuild_indices()
    }

    pub fn add_function(
        &mut self,
        def: crate::function::FunctionDef,
    ) -> Result<(), OntologyInvariantError> {
        if self.lookup.function_id_idx.contains_key(&def.id) {
            return Err(OntologyInvariantError::DuplicateCollectionId {
                kind: "function",
                id: def.id.to_string(),
            });
        }
        self.functions.push(def);
        self.rebuild_indices()
    }

    pub fn add_metric(
        &mut self,
        def: crate::metric::MetricDef,
    ) -> Result<(), OntologyInvariantError> {
        if self.lookup.metric_id_idx.contains_key(&def.id) {
            return Err(OntologyInvariantError::DuplicateCollectionId {
                kind: "metric",
                id: def.id.to_string(),
            });
        }
        self.metrics.push(def);
        self.rebuild_indices()
    }

    pub fn add_enrichment(
        &mut self,
        def: crate::enrichment::EnrichmentDef,
    ) -> Result<(), OntologyInvariantError> {
        if self.lookup.enrichment_id_idx.contains_key(&def.id) {
            return Err(OntologyInvariantError::DuplicateCollectionId {
                kind: "enrichment",
                id: def.id.to_string(),
            });
        }
        self.enrichments.push(def);
        self.rebuild_indices()
    }

    pub fn add_glossary_term(
        &mut self,
        def: crate::glossary::GlossaryTermDef,
    ) -> Result<(), OntologyInvariantError> {
        use crate::glossary::TermLifecycle;

        if self.lookup.glossary_term_id_idx.contains_key(&def.id) {
            return Err(OntologyInvariantError::DuplicateCollectionId {
                kind: "glossary_term",
                id: def.id.to_string(),
            });
        }
        // A deprecated term's `replaced_by` must point at a term that
        // already exists in the IR — otherwise NL-to-Cypher will route
        // user queries to a phantom successor. Self-replacement is
        // also rejected: it always produces an infinite redirect loop.
        if let TermLifecycle::Deprecated {
            replaced_by: Some(target),
            ..
        } = &def.lifecycle
        {
            if target == &def.id {
                return Err(OntologyInvariantError::InvalidReference {
                    kind: "glossary_term.replaced_by",
                    id: def.id.to_string(),
                    target: target.to_string(),
                });
            }
            if !self.lookup.glossary_term_id_idx.contains_key(target) {
                return Err(OntologyInvariantError::InvalidReference {
                    kind: "glossary_term.replaced_by",
                    id: def.id.to_string(),
                    target: target.to_string(),
                });
            }
        }
        self.glossary.push(def);
        self.rebuild_indices()
    }

    pub fn add_data_quality(
        &mut self,
        def: crate::data_quality::DataQualityDef,
    ) -> Result<(), OntologyInvariantError> {
        if self.lookup.data_quality_id_idx.contains_key(&def.id) {
            return Err(OntologyInvariantError::DuplicateCollectionId {
                kind: "data_quality",
                id: def.id.to_string(),
            });
        }
        self.data_quality.push(def);
        self.rebuild_indices()
    }

    /// Provenance is append-only — no duplicate check. Every
    /// assertion is a historical fact; the same subject can carry
    /// many provenance records (one per rewrite / re-derivation).
    pub fn add_provenance(&mut self, def: crate::provenance::ProvenanceDef) {
        self.provenance.push(def);
        // Intentionally no rebuild — provenance is not indexed.
    }

    /// Register a named NodeType-membership predicate. Referential
    /// integrity (`target_node_type_id` resolves; every
    /// `referenced_properties()` entry exists on the target) is
    /// asserted by `validate()` rather than at insert because the
    /// project flow lands segments alongside the node types they
    /// target — strict insert-time checks would force a two-pass
    /// commit.
    pub fn add_segment(
        &mut self,
        def: crate::segment::SegmentDef,
    ) -> Result<(), OntologyInvariantError> {
        if self.lookup.segment_id_idx.contains_key(&def.id) {
            return Err(OntologyInvariantError::DuplicateCollectionId {
                kind: "segment",
                id: def.id.to_string(),
            });
        }
        self.segments.push(def);
        self.rebuild_indices()
    }

    /// Record or refresh one source-table inventory row. Idempotent
    /// on the natural key `(source_id, table_name)` — re-
    /// introspection replaces the prior row in place. Callers that
    /// want delete semantics flip `status` to `Retracted` so the
    /// audit trail keeps historical `contributed_*` ids reachable.
    pub fn upsert_table_inventory_entry(
        &mut self,
        entry: crate::table_inventory::TableInventoryEntry,
    ) -> Result<(), OntologyInvariantError> {
        let key = (entry.source_id.clone(), entry.table_name.clone());
        match self.lookup.table_inventory_idx.get(&key) {
            Some(&idx) => {
                self.table_inventory[idx] = entry;
            }
            None => {
                self.table_inventory.push(entry);
            }
        }
        self.rebuild_indices()
    }

    pub fn add_object_mapping(
        &mut self,
        def: crate::mapping::ObjectMappingDef,
    ) -> Result<(), OntologyInvariantError> {
        if self.lookup.object_mapping_id_idx.contains_key(&def.id) {
            return Err(OntologyInvariantError::DuplicateCollectionId {
                kind: "object_mapping",
                id: def.id.to_string(),
            });
        }
        self.object_mappings.push(def);
        self.rebuild_indices()
    }

    pub fn add_link_mapping(
        &mut self,
        def: crate::mapping::LinkMappingDef,
    ) -> Result<(), OntologyInvariantError> {
        if self.lookup.link_mapping_id_idx.contains_key(&def.id) {
            return Err(OntologyInvariantError::DuplicateCollectionId {
                kind: "link_mapping",
                id: def.id.to_string(),
            });
        }
        self.link_mappings.push(def);
        self.rebuild_indices()
    }

    /// Append (or upsert by id) a column profile snapshot.
    ///
    /// Identity is `(source_id, relation, column)` — encoded into the
    /// stable `ColumnProfileId` via
    /// [`crate::column_profile::ColumnProfileDef::make_id`]. A
    /// re-snapshot of the same location *replaces* the previous entry
    /// rather than erroring on duplicate id, because the IR's
    /// contract is "always carry the most recent profile per
    /// location" — re-introspection should not require a delete first.
    pub fn add_column_profile(
        &mut self,
        def: crate::column_profile::ColumnProfileDef,
    ) {
        if let Some(&idx) = self.lookup.column_profile_id_idx.get(&def.id) {
            self.column_profiles[idx] = def;
        } else {
            self.column_profiles.push(def);
        }
        // `rebuild_indices` is infallible here — the upsert keeps id
        // uniqueness by construction, so no `DuplicateCollectionId`
        // can fire. We still call it to keep every other lookup map
        // (which we may have invalidated by mutating
        // `column_profiles`) in sync.
        if let Err(error) = self.rebuild_indices() {
            tracing::warn!(?error, "rebuild_indices unexpectedly failed during column-profile bulk update");
        }
    }

    /// Bulk-ingest every column entry of a [`SourceProfile`] for a
    /// given source. Each `(table, column)` pair becomes (or
    /// replaces) one [`ColumnProfileDef`] entry stamped with
    /// `sampled_at`.
    ///
    /// `source_id` is the same identifier the matching
    /// `ObjectMappingDef` uses, so the snapshot is queryable by the
    /// same key the rest of the IR addresses the source by.
    ///
    /// Returns the count of entries added or updated. Use this from
    /// the post-`analyze_*` pipeline so the IR captures the full
    /// distribution snapshot the kernel just computed without
    /// requiring a second round-trip to the source.
    pub fn ingest_source_profile(
        &mut self,
        source_id: &crate::mapping::SourceId,
        profile: &ox_core::source_schema::SourceProfile,
        sampled_at: chrono::DateTime<chrono::Utc>,
    ) -> usize {
        let entries = crate::column_profile::profile_to_column_defs(
            source_id, profile, sampled_at,
        );
        let n = entries.len();
        for entry in entries {
            self.add_column_profile(entry);
        }
        n
    }

    /// Ω-7: add a [`crate::value_range::ValueRangeSetDef`]. Optional
    /// overlap-check is not enforced at insert time — authors may
    /// intentionally commit a non-atomic state mid-edit — but the
    /// `find_overlaps()` method surfaces the issue for the UI to
    /// flag.
    pub fn add_value_range_set(
        &mut self,
        def: crate::value_range::ValueRangeSetDef,
    ) -> Result<(), OntologyInvariantError> {
        if self.lookup.value_range_set_id_idx.contains_key(&def.id) {
            return Err(OntologyInvariantError::DuplicateCollectionId {
                kind: "value_range_set",
                id: def.id.to_string(),
            });
        }
        self.value_range_sets.push(def);
        self.rebuild_indices()
    }

    /// Ω-5: add a [`crate::concept_map::ConceptMapDef`]. Referential
    /// integrity of source / target `CodeSystemId` fields is
    /// enforced here so a malformed map fails fast at insert.
    pub fn add_concept_map(
        &mut self,
        def: crate::concept_map::ConceptMapDef,
    ) -> Result<(), OntologyInvariantError> {
        if self.lookup.concept_map_id_idx.contains_key(&def.id) {
            return Err(OntologyInvariantError::DuplicateCollectionId {
                kind: "concept_map",
                id: def.id.to_string(),
            });
        }
        if !self.lookup.code_system_id_idx.contains_key(&def.source_system_id) {
            return Err(OntologyInvariantError::DuplicateCollectionId {
                kind: "concept_map_unknown_source_system",
                id: def.source_system_id.to_string(),
            });
        }
        if !self.lookup.code_system_id_idx.contains_key(&def.target_system_id) {
            return Err(OntologyInvariantError::DuplicateCollectionId {
                kind: "concept_map_unknown_target_system",
                id: def.target_system_id.to_string(),
            });
        }
        self.concept_maps.push(def);
        self.rebuild_indices()
    }

    /// Ω-4: add a [`crate::notation_pattern::NotationPatternDef`].
    /// Referential integrity of `CodeFromSet` components is
    /// enforced here — a component referencing a missing value set
    /// fails fast at insert time instead of at first parse.
    pub fn add_notation_pattern(
        &mut self,
        def: crate::notation_pattern::NotationPatternDef,
    ) -> Result<(), OntologyInvariantError> {
        if self.lookup.notation_pattern_id_idx.contains_key(&def.id) {
            return Err(OntologyInvariantError::DuplicateCollectionId {
                kind: "notation_pattern",
                id: def.id.to_string(),
            });
        }
        for component in &def.components {
            if let crate::notation_pattern::NotationComponentKind::CodeFromSet {
                value_set_id,
            } = &component.kind
                && !self.lookup.value_set_id_idx.contains_key(value_set_id)
            {
                return Err(OntologyInvariantError::DuplicateCollectionId {
                    kind: "notation_pattern_unknown_value_set",
                    id: value_set_id.to_string(),
                });
            }
        }
        self.notation_patterns.push(def);
        self.rebuild_indices()
    }

    /// Ω-2: add a [`ValueSetDef`]. Referential integrity of
    /// `ValueSetIncludeRule.system_id` against existing code
    /// systems is enforced here so a malformed composition fails
    /// fast rather than at expansion time.
    pub fn add_value_set(
        &mut self,
        def: crate::value_set::ValueSetDef,
    ) -> Result<(), OntologyInvariantError> {
        if self.lookup.value_set_id_idx.contains_key(&def.id) {
            return Err(OntologyInvariantError::DuplicateCollectionId {
                kind: "value_set",
                id: def.id.to_string(),
            });
        }
        for rule in &def.composition {
            if !self.lookup.code_system_id_idx.contains_key(&rule.system_id) {
                return Err(OntologyInvariantError::DuplicateCollectionId {
                    kind: "value_set_unknown_system",
                    id: rule.system_id.to_string(),
                });
            }
        }
        self.value_sets.push(def);
        self.rebuild_indices()
    }

    /// Remove a [`crate::glossary::GlossaryTermDef`] by id.
    pub fn remove_glossary_term(
        &mut self,
        id: &crate::glossary::GlossaryTermId,
    ) -> Result<(), OntologyInvariantError> {
        let before = self.glossary.len();
        self.glossary.retain(|t| &t.id != id);
        if self.glossary.len() == before {
            return Err(OntologyInvariantError::CollectionEntryNotFound {
                kind: "glossary_term",
                id: id.to_string(),
            });
        }
        self.rebuild_indices()
    }

    /// Remove a [`crate::rule::RuleDef`] by id. Parallel to
    /// [`OntologyIR::add_rule`] so the edit-log layer can express the
    /// full rule lifecycle through `OntologyEditOp` without reaching
    /// into the internal `rules` Vec.
    pub fn remove_rule(
        &mut self,
        id: &crate::action::RuleId,
    ) -> Result<(), OntologyInvariantError> {
        let before = self.rules.len();
        self.rules.retain(|r| &r.id != id);
        if self.rules.len() == before {
            return Err(OntologyInvariantError::CollectionEntryNotFound {
                kind: "rule",
                id: id.to_string(),
            });
        }
        self.rebuild_indices()
    }

    /// Remove an [`crate::mapping::ObjectMappingDef`] by id.
    pub fn remove_object_mapping(
        &mut self,
        id: &crate::mapping::ObjectMappingId,
    ) -> Result<(), OntologyInvariantError> {
        let before = self.object_mappings.len();
        self.object_mappings.retain(|m| &m.id != id);
        if self.object_mappings.len() == before {
            return Err(OntologyInvariantError::CollectionEntryNotFound {
                kind: "object_mapping",
                id: id.to_string(),
            });
        }
        self.rebuild_indices()
    }

    /// Remove a [`crate::mapping::LinkMappingDef`] by id.
    pub fn remove_link_mapping(
        &mut self,
        id: &crate::mapping::LinkMappingId,
    ) -> Result<(), OntologyInvariantError> {
        let before = self.link_mappings.len();
        self.link_mappings.retain(|m| &m.id != id);
        if self.link_mappings.len() == before {
            return Err(OntologyInvariantError::CollectionEntryNotFound {
                kind: "link_mapping",
                id: id.to_string(),
            });
        }
        self.rebuild_indices()
    }

    /// Remove a [`crate::notation_pattern::NotationPatternDef`] by id.
    pub fn remove_notation_pattern(
        &mut self,
        id: &crate::notation_pattern::NotationPatternId,
    ) -> Result<(), OntologyInvariantError> {
        let before = self.notation_patterns.len();
        self.notation_patterns.retain(|p| &p.id != id);
        if self.notation_patterns.len() == before {
            return Err(OntologyInvariantError::CollectionEntryNotFound {
                kind: "notation_pattern",
                id: id.to_string(),
            });
        }
        self.rebuild_indices()
    }

    /// Remove a [`crate::concept_map::ConceptMapDef`] by id.
    pub fn remove_concept_map(
        &mut self,
        id: &crate::concept_map::ConceptMapId,
    ) -> Result<(), OntologyInvariantError> {
        let before = self.concept_maps.len();
        self.concept_maps.retain(|m| &m.id != id);
        if self.concept_maps.len() == before {
            return Err(OntologyInvariantError::CollectionEntryNotFound {
                kind: "concept_map",
                id: id.to_string(),
            });
        }
        self.rebuild_indices()
    }

    /// Remove a [`crate::value_set::ValueSetDef`] by id.
    pub fn remove_value_set(
        &mut self,
        id: &crate::value_set::ValueSetId,
    ) -> Result<(), OntologyInvariantError> {
        let before = self.value_sets.len();
        self.value_sets.retain(|s| &s.id != id);
        if self.value_sets.len() == before {
            return Err(OntologyInvariantError::CollectionEntryNotFound {
                kind: "value_set",
                id: id.to_string(),
            });
        }
        self.rebuild_indices()
    }

    /// Remove a [`crate::code_system::CodeSystemDef`] by id.
    pub fn remove_code_system(
        &mut self,
        id: &crate::code_system::CodeSystemId,
    ) -> Result<(), OntologyInvariantError> {
        let before = self.code_systems.len();
        self.code_systems.retain(|s| &s.id != id);
        if self.code_systems.len() == before {
            return Err(OntologyInvariantError::CollectionEntryNotFound {
                kind: "code_system",
                id: id.to_string(),
            });
        }
        self.rebuild_indices()
    }

    /// Ω-1: add a [`CodeSystemDef`] to the terminology registry.
    ///
    /// Rebuild catches:
    /// - Duplicate `system.id`.
    /// - Duplicate [`CodedValueId`] across any existing system —
    ///   `coded_value_loc` is a global index.
    ///
    /// A malformed `broader_id` (pointing outside this system, or
    /// into a flat system) is caught in the semantic `validate()`
    /// pass, not here — structural uniqueness + referential
    /// integrity (the invariants that make lookups safe) is the
    /// only thing `add_code_system` enforces.
    pub fn add_code_system(
        &mut self,
        def: crate::code_system::CodeSystemDef,
    ) -> Result<(), OntologyInvariantError> {
        if self.lookup.code_system_id_idx.contains_key(&def.id) {
            return Err(OntologyInvariantError::DuplicateCollectionId {
                kind: "code_system",
                id: def.id.to_string(),
            });
        }
        self.code_systems.push(def);
        self.rebuild_indices()
    }

    // -------------------------------------------------------------------
    // O(1) by_id accessors. Each helper returns `Option<&XxxDef>`
    // so callers explicitly handle the missing case. Upstream
    // validators should prefer
    // these over `iter().find()` — the lookup maps are already
    // populated by `rebuild_indices`.
    // -------------------------------------------------------------------

    pub fn interface_by_id(
        &self,
        id: &crate::interface::InterfaceId,
    ) -> Option<&crate::interface::InterfaceDef> {
        self.lookup.interface_id_idx.get(id).map(|&i| &self.interfaces[i])
    }

    pub fn rule_by_id(
        &self,
        id: &crate::action::RuleId,
    ) -> Option<&crate::rule::RuleDef> {
        self.lookup.rule_id_idx.get(id).map(|&i| &self.rules[i])
    }

    pub fn action_by_id(
        &self,
        id: &crate::action::ActionId,
    ) -> Option<&crate::action::ActionDef> {
        self.lookup.action_id_idx.get(id).map(|&i| &self.actions[i])
    }

    pub fn function_by_id(
        &self,
        id: &crate::function::FunctionId,
    ) -> Option<&crate::function::FunctionDef> {
        self.lookup.function_id_idx.get(id).map(|&i| &self.functions[i])
    }

    pub fn metric_by_id(
        &self,
        id: &crate::metric::MetricId,
    ) -> Option<&crate::metric::MetricDef> {
        self.lookup.metric_id_idx.get(id).map(|&i| &self.metrics[i])
    }

    pub fn enrichment_by_id(
        &self,
        id: &crate::enrichment::EnrichmentId,
    ) -> Option<&crate::enrichment::EnrichmentDef> {
        self.lookup.enrichment_id_idx.get(id).map(|&i| &self.enrichments[i])
    }

    pub fn glossary_term_by_id(
        &self,
        id: &crate::glossary::GlossaryTermId,
    ) -> Option<&crate::glossary::GlossaryTermDef> {
        self.lookup.glossary_term_id_idx.get(id).map(|&i| &self.glossary[i])
    }

    /// Resolve a free-form phrase to the glossary term that matches
    /// it across any locale. Walks `term`, `display_name`, and
    /// `aliases` (their `default` plus every translation) for a
    /// case-insensitive trim-equal match, then follows
    /// [`crate::glossary::TermLifecycle::Deprecated::replaced_by`]
    /// chains so the caller always lands on a currently-active
    /// successor.
    ///
    /// Used by NL→Cypher / schema-RAG routing: a Korean operator
    /// querying with `"고객"` resolves to the same term identity as
    /// an English operator querying with `"Customer"`, and a query
    /// against a deprecated label silently routes through to the
    /// successor without surfacing the deprecation cycle.
    ///
    /// **Match priority** (deterministic across glossary order):
    /// 1. Active term whose canonical `term` equals the phrase.
    /// 2. Active term whose `display_name` equals the phrase.
    /// 3. Active term whose `aliases` contains the phrase.
    /// 4. Deprecated term — same hierarchy. The resolver still
    ///    returns the *successor* via the replacement chain, but
    ///    matches strictly through the active surface first so a
    ///    coincidental alias collision between an active and a
    ///    retired concept resolves to the active one.
    ///
    /// Cycle protection: redirect chains stop after
    /// `MAX_REPLACEMENT_HOPS` so a malformed cycle in stored data
    /// can never hang the resolver.
    pub fn glossary_term_by_phrase(
        &self,
        phrase: &str,
    ) -> Option<&crate::glossary::GlossaryTermDef> {
        let needle = phrase.trim().to_lowercase();
        if needle.is_empty() {
            return None;
        }
        // Visit active terms first, then deprecated. Within each
        // bucket the canonical `term` outranks `display_name`, which
        // outranks `aliases`. The first match wins; downstream
        // replacement-chain walking turns a deprecated hit into its
        // active successor automatically.
        let buckets: [bool; 2] = [true, false];
        for prefer_active in buckets {
            for slot in [
                GlossarySlot::Term,
                GlossarySlot::DisplayName,
                GlossarySlot::Alias,
            ] {
                if let Some(found) = self.glossary.iter().find(|t| {
                    t.is_active() == prefer_active
                        && slot_matches(t, slot, &needle)
                }) {
                    return Some(self.follow_replacement_chain(found));
                }
            }
        }
        None
    }

    /// Walk through `Deprecated::replaced_by` until reaching a term
    /// whose lifecycle is *not* a redirect, or hitting the cycle
    /// guard. Always returns an existing term — the integrity layer
    /// rejects unknown / self-referencing replacements at insert.
    fn follow_replacement_chain<'a>(
        &'a self,
        start: &'a crate::glossary::GlossaryTermDef,
    ) -> &'a crate::glossary::GlossaryTermDef {
        const MAX_REPLACEMENT_HOPS: usize = 8;
        let mut current = start;
        for _ in 0..MAX_REPLACEMENT_HOPS {
            let Some(next_id) = current.replaced_by() else {
                return current;
            };
            let Some(next) = self.glossary_term_by_id(next_id) else {
                return current;
            };
            if next.id == current.id {
                return current;
            }
            current = next;
        }
        current
    }

    pub fn data_quality_by_id(
        &self,
        id: &crate::data_quality::DataQualityId,
    ) -> Option<&crate::data_quality::DataQualityDef> {
        self.lookup
            .data_quality_id_idx
            .get(id)
            .map(|&i| &self.data_quality[i])
    }

    pub fn object_mapping_by_id(
        &self,
        id: &crate::mapping::ObjectMappingId,
    ) -> Option<&crate::mapping::ObjectMappingDef> {
        self.lookup
            .object_mapping_id_idx
            .get(id)
            .map(|&i| &self.object_mappings[i])
    }

    pub fn link_mapping_by_id(
        &self,
        id: &crate::mapping::LinkMappingId,
    ) -> Option<&crate::mapping::LinkMappingDef> {
        self.lookup
            .link_mapping_id_idx
            .get(id)
            .map(|&i| &self.link_mappings[i])
    }

    /// Ω-1 terminology — O(1) lookup for a code system by id.
    pub fn code_system_by_id(
        &self,
        id: &crate::code_system::CodeSystemId,
    ) -> Option<&crate::code_system::CodeSystemDef> {
        self.lookup
            .code_system_id_idx
            .get(id)
            .map(|&i| &self.code_systems[i])
    }

    /// Ω-2 — O(1) lookup for a value set by id.
    pub fn value_set_by_id(
        &self,
        id: &crate::value_set::ValueSetId,
    ) -> Option<&crate::value_set::ValueSetDef> {
        self.lookup
            .value_set_id_idx
            .get(id)
            .map(|&i| &self.value_sets[i])
    }

    /// Ω-4 — O(1) lookup for a notation pattern by id.
    pub fn notation_pattern_by_id(
        &self,
        id: &crate::notation_pattern::NotationPatternId,
    ) -> Option<&crate::notation_pattern::NotationPatternDef> {
        self.lookup
            .notation_pattern_id_idx
            .get(id)
            .map(|&i| &self.notation_patterns[i])
    }

    /// Ω-5 — O(1) lookup for a concept map by id.
    pub fn concept_map_by_id(
        &self,
        id: &crate::concept_map::ConceptMapId,
    ) -> Option<&crate::concept_map::ConceptMapDef> {
        self.lookup
            .concept_map_id_idx
            .get(id)
            .map(|&i| &self.concept_maps[i])
    }

    /// Ω-7 — O(1) lookup for a value range set by id.
    pub fn value_range_set_by_id(
        &self,
        id: &crate::value_range::ValueRangeSetId,
    ) -> Option<&crate::value_range::ValueRangeSetDef> {
        self.lookup
            .value_range_set_id_idx
            .get(id)
            .map(|&i| &self.value_range_sets[i])
    }

    /// O(1) lookup for a column profile by id.
    pub fn column_profile_by_id(
        &self,
        id: &crate::column_profile::ColumnProfileId,
    ) -> Option<&crate::column_profile::ColumnProfileDef> {
        self.lookup
            .column_profile_id_idx
            .get(id)
            .map(|&i| &self.column_profiles[i])
    }

    /// Ω-1 terminology — O(1) lookup for a coded value by id. The
    /// `CodedValueId` namespace is global across all systems in
    /// this ontology (rebuild_indices enforces uniqueness), so the
    /// caller does not need to know which system owns the code.
    pub fn coded_value_by_id(
        &self,
        id: &crate::code_system::CodedValueId,
    ) -> Option<(
        &crate::code_system::CodeSystemDef,
        &crate::code_system::CodedValue,
    )> {
        self.lookup
            .coded_value_loc
            .get(id)
            .map(|&(sys_idx, code_idx)| {
                let system = &self.code_systems[sys_idx];
                let code = &system.codes[code_idx];
                (system, code)
            })
    }

    /// Resolve a node's label from its ID. O(1).
    pub fn node_label(&self, node_id: &str) -> Option<&str> {
        self.lookup
            .node_id_idx
            .get(node_id)
            .map(|&i| self.node_types[i].label.as_str())
    }

    /// Look up a node type by its stable ID. O(1).
    pub fn node_by_id(&self, node_id: &str) -> Option<&NodeTypeDef> {
        self.lookup
            .node_id_idx
            .get(node_id)
            .map(|&i| &self.node_types[i])
    }

    /// Look up a node type by its label. O(1).
    pub fn node_by_label(&self, label: &str) -> Option<&NodeTypeDef> {
        self.lookup
            .node_label_idx
            .get(label)
            .map(|&i| &self.node_types[i])
    }

    /// Look up a property by its stable ID across all node types. O(1).
    /// Returns the owning node and the property.
    pub fn property_by_id(&self, prop_id: &str) -> Option<(&NodeTypeDef, &PropertyDef)> {
        self.lookup
            .prop_id_loc
            .get(prop_id)
            .map(|&(ni, pi)| (&self.node_types[ni], &self.node_types[ni].properties[pi]))
    }

    /// Look up an edge type by its stable ID. O(1).
    pub fn edge_by_id(&self, edge_id: &str) -> Option<&EdgeTypeDef> {
        self.lookup
            .edge_id_idx
            .get(edge_id)
            .map(|&i| &self.edge_types[i])
    }

    /// Find a property by ID within a specific property list.
    pub fn property_in<'a>(
        &self,
        properties: &'a [PropertyDef],
        prop_id: &str,
    ) -> Option<&'a PropertyDef> {
        properties.iter().find(|p| p.id == prop_id)
    }

    // `unknown_labels_in_query(&OntologyIR, &QueryIR)` lives in
    // `ox_query_ir::ontology_conformance` rather than here — taking
    // `&QueryIR` on an `OntologyIR` method would force a circular
    // dependency. The free-function form calls
    // `ontology.node_by_label` / `ontology.edge_types` through the
    // public API.

    // -----------------------------------------------------------------------
    // Schema RAG — natural language descriptions for embedding + compact schema
    // -----------------------------------------------------------------------

    /// Convert each node+edge into a natural language description for semantic embedding.
    /// Each entry is `(stable_id, natural_language_text)`.
    pub fn to_schema_entries(&self) -> Vec<(String, String)> {
        let mut entries = Vec::new();

        for node in &self.node_types {
            // Collect connected edges
            let outgoing: Vec<&str> = self
                .edge_types
                .iter()
                .filter(|e| e.source_node_id == node.id)
                .map(|e| self.node_label(e.target_node_id.as_ref()).unwrap_or("?"))
                .collect();
            let incoming: Vec<(&str, &str)> = self
                .edge_types
                .iter()
                .filter(|e| e.target_node_id == node.id)
                .map(|e| {
                    let src = self.node_label(e.source_node_id.as_ref()).unwrap_or("?");
                    (src, e.label.as_str())
                })
                .collect();

            let props: Vec<&str> = node.properties.iter().map(|p| p.name.as_str()).collect();

            let desc = node.description.as_str();
            let mut text = format!("{}: {} Properties: {}.", node.label, desc, props.join(", "));

            if !outgoing.is_empty() {
                text.push_str(&format!(" Connected to: {}.", outgoing.join(", ")));
            }
            if !incoming.is_empty() {
                let rels: Vec<String> = incoming
                    .iter()
                    .map(|(src, edge)| format!("{src} via {edge}"))
                    .collect();
                text.push_str(&format!(" Referenced by: {}.", rels.join(", ")));
            }

            entries.push((node.id.as_ref().to_string(), text));
        }

        entries
    }

    /// Render a tiered schema view. Pick the smallest tier that gives
    /// the LLM enough signal — every byte costs tokens.
    ///
    /// - `Labels` (~10 tokens/node): list of node and edge label names
    ///   only. Use during label discovery / RAG selection.
    /// - `Structural` (~40 tokens/node): per-node property names and
    ///   1-hop edge connectivity. Use when the model needs to plan a
    ///   query shape but does not yet need property types or
    ///   descriptions.
    /// - `Detailed` (~120 tokens/node): full property types, nullable,
    ///   descriptions, edge cardinality, edge properties. Use only for
    ///   the subset of labels actually needed in the final query.
    pub fn schema_view(&self, view: SchemaView, node_labels: &[&str]) -> serde_json::Value {
        match view {
            SchemaView::Labels => self.labels_view(node_labels),
            SchemaView::Structural => self.structural_view(node_labels),
            SchemaView::Detailed => self.compact_schema(node_labels),
        }
    }

    /// Tier 1 — node and edge label lists only.
    fn labels_view(&self, node_labels: &[&str]) -> serde_json::Value {
        use std::collections::HashSet;
        let selected: HashSet<&str> = node_labels.iter().copied().collect();

        let nodes: Vec<&str> = self
            .node_types
            .iter()
            .filter(|n| selected.contains(n.label.as_str()))
            .map(|n| n.label.as_str())
            .collect();

        let edges: Vec<&str> = self
            .edge_types
            .iter()
            .filter(|e| {
                let src = self.node_label(e.source_node_id.as_ref()).unwrap_or("?");
                let tgt = self.node_label(e.target_node_id.as_ref()).unwrap_or("?");
                selected.contains(src) || selected.contains(tgt)
            })
            .map(|e| e.label.as_str())
            .collect();

        serde_json::json!({ "nodes": nodes, "edges": edges })
    }

    /// Tier 2 — per-node property names + 1-hop edge connectivity.
    /// Drops type metadata, descriptions, cardinality.
    fn structural_view(&self, node_labels: &[&str]) -> serde_json::Value {
        use std::collections::HashSet;
        let selected: HashSet<&str> = node_labels.iter().copied().collect();

        let mut nodes = serde_json::Map::new();
        for node in &self.node_types {
            if !selected.contains(node.label.as_str()) {
                continue;
            }
            let props: Vec<&str> = node.properties.iter().map(|p| p.name.as_str()).collect();
            nodes.insert(
                node.label.as_str().to_string(),
                serde_json::json!({ "properties": props }),
            );
        }

        let mut edges = serde_json::Map::new();
        for edge in &self.edge_types {
            let src = self.node_label(edge.source_node_id.as_ref()).unwrap_or("?");
            let tgt = self.node_label(edge.target_node_id.as_ref()).unwrap_or("?");
            if !selected.contains(src) && !selected.contains(tgt) {
                continue;
            }
            edges.insert(
                edge.label.as_str().to_string(),
                serde_json::json!({ "source": src, "target": tgt }),
            );
        }

        serde_json::json!({ "nodes": nodes, "edges": edges })
    }

    /// Build a compact JSON schema for a subset of nodes (identified by labels).
    /// Includes full property descriptions and edge connections — minimal but complete
    /// for LLM query translation.
    pub fn compact_schema(&self, node_labels: &[&str]) -> serde_json::Value {
        use std::collections::HashSet;
        let selected: HashSet<&str> = node_labels.iter().copied().collect();

        let mut nodes = serde_json::Map::new();
        let mut edges = serde_json::Map::new();

        for node in &self.node_types {
            if !selected.contains(node.label.as_str()) {
                continue;
            }
            let mut props = serde_json::Map::new();
            for p in &node.properties {
                let desc = p.description.as_str();
                let nullable = if p.nullable { ", nullable" } else { "" };
                let hints = property_hints(p);
                props.insert(
                    p.name.to_string(),
                    serde_json::Value::String(
                        format!("{}{}{} {}", p.property_type, nullable, hints, desc)
                            .trim()
                            .to_string(),
                    ),
                );
            }
            let mut node_obj = serde_json::Map::new();
            let mut node_description = String::new();
            if !node.description.is_empty() {
                node_description.push_str(node.description.default_str());
            }
            if node.deprecated_at.is_some() {
                if !node_description.is_empty() {
                    node_description.push_str(" — ");
                }
                node_description.push_str("[DEPRECATED]");
            }
            if !node_description.is_empty() {
                node_obj.insert(
                    "description".into(),
                    serde_json::Value::String(node_description),
                );
            }
            node_obj.insert("properties".into(), serde_json::Value::Object(props));
            nodes.insert(
                node.label.as_str().to_string(),
                serde_json::Value::Object(node_obj),
            );
        }

        // Include edges where both source and target are in the selected set
        for edge in &self.edge_types {
            let src_label = self.node_label(edge.source_node_id.as_ref()).unwrap_or("?");
            let tgt_label = self.node_label(edge.target_node_id.as_ref()).unwrap_or("?");
            if selected.contains(src_label) || selected.contains(tgt_label) {
                let mut edge_obj = serde_json::Map::new();
                edge_obj.insert(
                    "source".into(),
                    serde_json::Value::String(src_label.to_string()),
                );
                edge_obj.insert(
                    "target".into(),
                    serde_json::Value::String(tgt_label.to_string()),
                );
                edge_obj.insert(
                    "cardinality".into(),
                    serde_json::Value::String(format!("{:?}", edge.cardinality)),
                );
                // Surface functional endpoint roles so the LLM sees that
                // MANAGES carries "manager"/"direct_report" semantics
                // rather than treating the edge label as the only hint.
                if let Some(role) = &edge.source_role {
                    edge_obj.insert(
                        "source_role".into(),
                        serde_json::Value::String(role.clone()),
                    );
                }
                if let Some(role) = &edge.target_role {
                    edge_obj.insert(
                        "target_role".into(),
                        serde_json::Value::String(role.clone()),
                    );
                }
                let mut edge_description = String::new();
                if !edge.description.is_empty() {
                    edge_description.push_str(edge.description.default_str());
                }
                if edge.deprecated_at.is_some() {
                    if !edge_description.is_empty() {
                        edge_description.push_str(" — ");
                    }
                    edge_description.push_str("[DEPRECATED]");
                }
                if !edge_description.is_empty() {
                    edge_obj.insert(
                        "description".into(),
                        serde_json::Value::String(edge_description),
                    );
                }
                if !edge.properties.is_empty() {
                    let props: Vec<String> =
                        edge.properties.iter().map(|p| p.name.to_string()).collect();
                    edge_obj.insert("properties".into(), serde_json::json!(props));
                }
                edges.insert(
                    edge.label.as_str().to_string(),
                    serde_json::Value::Object(edge_obj),
                );
            }
        }

        serde_json::json!({
            "nodes": nodes,
            "edges": edges,
        })
    }

    /// Get 1-hop neighbor labels for a given node label.
    pub fn neighbor_labels(&self, label: &str) -> Vec<&str> {
        let node = match self.node_by_label(label) {
            Some(n) => n,
            None => return vec![],
        };
        let mut neighbors = Vec::new();
        for edge in &self.edge_types {
            if edge.source_node_id == node.id
                && let Some(tgt) = self.node_label(edge.target_node_id.as_ref())
            {
                neighbors.push(tgt);
            }
            if edge.target_node_id == node.id
                && let Some(src) = self.node_label(edge.source_node_id.as_ref())
            {
                neighbors.push(src);
            }
        }
        neighbors.sort_unstable();
        neighbors.dedup();
        neighbors
    }
}

/// Build a bracketed hint suffix carrying Phase A semantic flags for a
/// Labelled slot on a `GlossaryTermDef` that the phrase resolver
/// inspects. The order in
/// [`OntologyIR::glossary_term_by_phrase`]'s match cascade —
/// `Term → DisplayName → Alias` — is the canonical SKOS preference
/// order: a phrase that hits the canonical preferred label outranks
/// a phrase that hits the same term only through an alias.
#[derive(Debug, Clone, Copy)]
enum GlossarySlot {
    Term,
    DisplayName,
    Alias,
}

fn slot_matches(
    term: &crate::glossary::GlossaryTermDef,
    slot: GlossarySlot,
    needle_lower: &str,
) -> bool {
    match slot {
        GlossarySlot::Term => localized_text_contains_ci(&term.term, needle_lower),
        GlossarySlot::DisplayName => {
            localized_text_contains_ci(&term.display_name, needle_lower)
        }
        GlossarySlot::Alias => term
            .aliases
            .iter()
            .any(|alias| localized_text_contains_ci(alias, needle_lower)),
    }
}

fn localized_text_contains_ci(
    text: &ox_core::i18n::LocalizedText,
    needle_lower: &str,
) -> bool {
    if text.default.to_lowercase() == needle_lower {
        return true;
    }
    text.translations
        .values()
        .any(|v| v.to_lowercase() == needle_lower)
}

/// property: `", localized"`, `", deprecated"`, `", min N"`, `", max N"`.
/// Empty string when the property has no special flags. Kept on one line
/// so it slots into `compact_schema` without bloating the per-property
/// token budget — an LLM consuming this view gets the governance hints
/// inline rather than having to cross-reference a sidecar table.
fn property_hints(p: &PropertyDef) -> String {
    let mut hints: Vec<String> = Vec::new();
    if p.is_localized {
        hints.push("localized".into());
    }
    if let Some(min) = p.min_count {
        hints.push(format!("min {min}"));
    }
    if let Some(max) = p.max_count {
        hints.push(format!("max {max}"));
    }
    if p.deprecated_at.is_some() {
        hints.push("deprecated".into());
    }
    if hints.is_empty() {
        String::new()
    } else {
        format!(", {}", hints.join(", "))
    }
}
