//! `ObjectMappingDef` — binding from one `NodeTypeDef` to one
//! physical relation.
//!
//! The shape is intentionally R2RML-inspired but Rust-native. A
//! mapping carries:
//!
//! - which node type it fulfils,
//! - which source relation it reads from,
//! - how every property of the node is materialised,
//! - how the planner should narrow the scan (row filter, workspace
//!   scope), when the mapping is active (temporal window), and whether
//!   the graph cache may help.
//!
//! Multi-mapping semantics (one node type, multiple mappings) are
//! resolved by the planner using `precedence` — the highest value
//! wins on conflicts, with `DISTINCT ON (primary_key_columns)`
//! applied across the `UNION ALL` of every mapping's scan.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::ir::NodeTypeId;
use crate::mapping::property::PropertyMappingDef;
use crate::mapping::refs::{
    CacheHintKind, ColumnRef, ObjectMappingId, SourceId, SourceRelationKind,
};

/// Binding from a `NodeTypeDef` to a physical relation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
pub struct ObjectMappingDef {
    /// Stable identifier for this mapping. Used in audit trails,
    /// cache invalidation, and mapping-level diagnostics.
    pub id: ObjectMappingId,

    /// Node type this mapping fulfils. The planner expands a
    /// `MATCH (n:Label)` to every mapping whose `node_type_id`
    /// resolves to that label, respecting `precedence`.
    pub node_type_id: NodeTypeId,

    /// Source the relation lives in. Paired with `relation`, this is
    /// the (source, name) tuple the adapter receives.
    pub source_id: SourceId,

    /// The physical relation name inside the source
    /// (`public.customers`, a Mongo collection, the inline `records`
    /// table for CSV).
    pub relation: String,

    /// What kind of source object the relation points at — the
    /// planner treats `View` and `Collection` differently from
    /// `Table` on write paths.
    #[serde(default)]
    pub relation_kind: SourceRelationKind,

    /// Column(s) that form the primary key in the source. Multi-
    /// column PKs are supported so that DISTINCT-ON dedup works on
    /// legacy composite keys. `None` = no PK — the planner cannot
    /// dedup multi-mapping unions for this node type and surfaces
    /// that as a warning.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub primary_key_columns: Vec<ColumnRef>,

    /// Optional row filter pushed into the `TableProvider::scan`.
    /// Expressed in the source's dialect — the planner does NOT
    /// translate across dialects. Typically used for soft-delete
    /// tombstones and per-mapping sub-setting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub row_filter: Option<String>,

    /// How each property is materialised. The planner validates at
    /// registration that every property on the node type is either
    /// mapped here or declared derived (via a `DerivedProperty`
    /// function).
    pub property_mappings: Vec<PropertyMappingDef>,

    /// Partitioning columns the source enforces. When non-empty the
    /// `SemanticGuardValidator` rejects queries that lack a literal
    /// predicate on at least one of these columns — translates the
    /// `require_partition_filter=true` contract from BigQuery /
    /// Snowflake / Hive into a pre-flight check instead of a
    /// per-query round-trip rejection.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub partition_columns: Vec<ColumnRef>,

    /// Column carrying the workspace id in the source, when present.
    /// The federation planner injects
    /// `workspace_scope = $_ws_id` into every scan on the mapping
    /// so RLS-inside-the-source is never the only gate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_scope: Option<ColumnRef>,

    /// Higher precedence wins in multi-mapping dedup. A new mapping
    /// that shadows an older one simply uses a higher number; the
    /// oldest mapping can stay registered for audit purposes. Ties
    /// resolve by `id` ascending so the resolved order is independent
    /// of insertion sequence.
    #[serde(default)]
    pub precedence: u32,

    /// When this mapping became (or will become) authoritative. A
    /// planner evaluating a query with `ontology_valid_at = t`
    /// rejects the mapping when `t < valid_from`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_from: Option<DateTime<Utc>>,

    /// When this mapping stops being authoritative. Open-ended when
    /// `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_to: Option<DateTime<Utc>>,

    /// Graph-cache participation for this mapping.
    #[serde(default)]
    pub cache_hint: CacheHintKind,
}

impl ObjectMappingDef {
    /// Construct a minimal mapping — no filters, no cache, highest
    /// precedence (so it wins any dedup). Suited to tests and to
    /// bootstrap paths that register a freshly-introspected source.
    pub fn new(
        id: impl Into<ObjectMappingId>,
        node_type_id: impl Into<NodeTypeId>,
        source_id: impl Into<SourceId>,
        relation: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            node_type_id: node_type_id.into(),
            source_id: source_id.into(),
            relation: relation.into(),
            relation_kind: SourceRelationKind::default(),
            primary_key_columns: Vec::new(),
            row_filter: None,
            property_mappings: Vec::new(),
            partition_columns: Vec::new(),
            workspace_scope: None,
            precedence: u32::MAX,
            valid_from: None,
            valid_to: None,
            cache_hint: CacheHintKind::default(),
        }
    }

    /// Is the mapping active at `at`? Returns `true` when `at` is
    /// within (or the mapping has no bound on) the `valid_*` window.
    /// Convenience for planner logic that walks the mapping list.
    pub fn is_valid_at(&self, at: DateTime<Utc>) -> bool {
        let after_start = self.valid_from.is_none_or(|f| at >= f);
        let before_end = self.valid_to.is_none_or(|t| at < t);
        after_start && before_end
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_defaults_are_maximum_precedence_and_open_window() {
        let m = ObjectMappingDef::new("om-1", "nt-user", "pg-main", "users");
        assert_eq!(m.precedence, u32::MAX);
        assert!(m.valid_from.is_none() && m.valid_to.is_none());
        assert!(matches!(m.cache_hint, CacheHintKind::None));
    }

    #[test]
    fn is_valid_at_handles_open_and_closed_windows() {
        let mut m = ObjectMappingDef::new("om-1", "nt-user", "pg-main", "users");
        assert!(m.is_valid_at(Utc::now())); // wide open

        let t0 = Utc::now();
        let t1 = t0 + chrono::Duration::hours(1);
        m.valid_from = Some(t0);
        m.valid_to = Some(t1);

        assert!(m.is_valid_at(t0 + chrono::Duration::minutes(30)));
        assert!(!m.is_valid_at(t0 - chrono::Duration::minutes(1)));
        assert!(!m.is_valid_at(t1)); // half-open, upper exclusive
    }

    #[test]
    fn roundtrips_through_json_without_losing_cache_hint() {
        let mut m = ObjectMappingDef::new("om-1", "nt-user", "pg-main", "users");
        m.cache_hint = CacheHintKind::GraphCache {
            ttl_seconds: 300,
            schedule: Some("*/5 * * * *".into()),
        };
        let j = serde_json::to_value(&m).unwrap();
        let back: ObjectMappingDef = serde_json::from_value(j).unwrap();
        assert_eq!(back, m);
    }
}
