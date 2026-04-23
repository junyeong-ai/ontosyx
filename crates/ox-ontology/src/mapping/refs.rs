//! Reference / identifier types used by the Phase 4 mapping layer.
//!
//! These primitives sit below `ObjectMappingDef`, `LinkMappingDef`, and
//! `PropertyMappingDef` — they describe *where* a value lives in a
//! physical source (a column in a table, a JSON path inside a
//! document, an external source id) without committing to a specific
//! dialect.
//!
//! The newtypes here use `ox_core::define_id_newtype!` so the
//! identity contract (Serde transparent, `Deref<Target=str>`,
//! comparable against `str`) matches every other `XxxId` in the
//! platform.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Identifier newtypes
// ---------------------------------------------------------------------------

ox_core::define_id_newtype!(
    /// Stable identifier for a configured data source (a Postgres
    /// connection, a Snowflake warehouse, a CSV file, etc.). Wire
    /// format is a bare string, so the id survives serde round-trips
    /// unchanged.
    ///
    /// Construction: prefer [`SourceId::from_source_config`] — the
    /// canonical rule is `{source_type}:{fingerprint}`, which keeps
    /// identity deterministic across reanalysis and prevents two
    /// sources of different kinds that happen to share a
    /// fingerprint from colliding. The bare `::new` constructor
    /// stays for tests and for adapters that synthesise an id
    /// outside the `SourceConfig` lifecycle.
    SourceId
);

impl SourceId {
    /// Canonical `{kind}:{fingerprint}` identity derived from a
    /// [`crate::design_project::SourceConfig`]. When the config has
    /// no fingerprint yet (e.g. before analysis), the source_type
    /// alone forms the id — those ids become concrete once the
    /// analyzer stamps a fingerprint and the project is re-saved.
    ///
    /// This is the ONE place that encodes the identity rule so
    /// callers across the workspace agree on the format. Helpers
    /// that previously built ids inline (e.g. the ambiguity path
    /// in `ox-api::routes::projects::helpers::source`) go through
    /// this method.
    pub fn from_source_config(config: &crate::design_project::SourceConfig) -> Self {
        match &config.source_fingerprint {
            Some(fp) => Self::new(format!("{}:{}", config.source_type, fp)),
            None => Self::new(config.source_type.to_string()),
        }
    }
}

ox_core::define_id_newtype!(
    /// Stable identifier for an `ObjectMappingDef` — the binding
    /// between one `NodeTypeDef` and one physical relation.
    ObjectMappingId
);

ox_core::define_id_newtype!(
    /// Stable identifier for a `LinkMappingDef` — the binding between
    /// one `EdgeTypeDef` and the physical relation(s) that supply
    /// edges of that type.
    LinkMappingId
);

// ---------------------------------------------------------------------------
// Column / table references
// ---------------------------------------------------------------------------

/// Qualified reference to a column within a source relation.
///
/// `relation` is the relation name (`public.customers`, a Mongo
/// collection, a CSV inline `records` relation). `column` is the
/// physical column name as it appears at the source — the adapter
/// layer applies any dialect quoting when it renders the scan plan.
#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct ColumnRef {
    pub relation: String,
    pub column: String,
}

impl ColumnRef {
    pub fn new(relation: impl Into<String>, column: impl Into<String>) -> Self {
        Self {
            relation: relation.into(),
            column: column.into(),
        }
    }
}

impl std::fmt::Display for ColumnRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}", self.relation, self.column)
    }
}

/// Source-side shape a mapping can bind to. `Table` is the default,
/// `View` keeps a strong distinction from it so the planner can
/// refuse write operations on views without sniffing the source
/// catalog. `Collection` covers document stores (MongoDB), `File`
/// covers filesystem-backed sources (CSV, JSON).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SourceRelationKind {
    #[default]
    Table,
    View,
    Collection,
    File,
}

/// Fully qualified reference to a relation in a specific source.
///
/// Used by `LinkMappingDef` to describe the bridge relation and to
/// name an endpoint's owning relation for federated edges.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SourceRelationRef {
    pub source_id: SourceId,
    pub relation: String,
    #[serde(default)]
    pub kind: SourceRelationKind,
}

// ---------------------------------------------------------------------------
// Cache hint
// ---------------------------------------------------------------------------

/// Per-mapping hint for the graph-cache backend (ADR 0004). The
/// planner treats `None` as "never cache"; `GraphCache` is an opt-in
/// that names a freshness window and an explicit refresh cadence.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum CacheHintKind {
    /// No cache participation — every query for this mapping routes
    /// to the live source.
    #[default]
    None,
    /// The planner may serve reads for this mapping from the graph
    /// cache when the cache is fresh enough. `ttl_seconds` is the
    /// freshness budget; beyond it the cache falls back to source.
    GraphCache {
        #[serde(default)]
        ttl_seconds: u64,
        /// Optional cron-style schedule that invalidates / refreshes
        /// the cache entry. Stored as free-form text and validated
        /// at registration time; `None` means manual refresh only.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        schedule: Option<String>,
    },
}
