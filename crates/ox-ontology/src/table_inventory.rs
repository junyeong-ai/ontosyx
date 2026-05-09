//! `TableInventoryEntry` — first-class record of every source
//! table the project has touched.
//!
//! Without this collection there's no canonical answer to "which
//! tables did this project bring in, and which NodeTypes /
//! EdgeTypes did each contribute to?". `object_mappings` (relation
//! → NodeType) only tells half the story; the inventory adds the
//! "imported but never mapped" / "available but never imported"
//! axes the source-as-first-class UX needs.
//!
//! `TableInventoryEntry` carries that axis at IR-level so:
//!
//! - the FE can render "this NodeType reads 5 tables, 2 columns are
//!   not yet mapped to properties, table `dim_segment` is in the
//!   source surface but not imported" without re-introspecting,
//! - per-table re-introspect and `AnalyzeSelection::Reduce` operate
//!   against an authoritative inventory (the schema fingerprint
//!   pins a stable identity even after table renames), and
//! - the Domain-Context Change Log can answer "where did this node
//!   come from?" with one IR lookup rather than a multi-collection
//!   walk.
//!
//! The entry is intentionally append-and-update only: the cron
//! cleanup that reaps dropped sources rewrites `included = false`
//! rather than deleting the row, so historical attribution survives
//! a retraction.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::ir::{EdgeTypeId, NodeTypeId};
use crate::mapping::SourceId;

/// Why a table is in the inventory — the operator's import intent.
///
/// `Imported` is the dominant case: the operator picked the table
/// during introspection and the kernel actually pulled its schema /
/// profile. `AvailableButNotImported` records a table the source
/// surface advertised but the operator declined; it is kept on the
/// inventory so the Source Inspector can offer a one-click
/// "extend with this table" without re-introspecting. `Retracted`
/// is the terminal state for a table that was previously imported
/// and later dropped via `AnalyzeSelection::Reduce` — the audit
/// trail keeps the `contributed_*` ids of the now-deleted entities
/// so historical queries can resolve them.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum TableInventoryStatus {
    #[default]
    Imported,
    AvailableButNotImported,
    Retracted,
}

/// One row of the project's source-table inventory.
///
/// `(source_id, table_name)` is the natural key — the IR's
/// `add_table_inventory_entry` upserts on the pair so re-introspection
/// against an unchanged source is idempotent. `schema_fingerprint`
/// captures the table's structural shape at the time of last
/// introspection; a fingerprint mismatch on a later run signals
/// schema drift that the FE surfaces as a "this source moved on"
/// banner.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
pub struct TableInventoryEntry {
    pub source_id: SourceId,
    pub table_name: String,
    /// Stable structural digest — empty when the entry pre-dates
    /// fingerprint capture. New code always populates it.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub schema_fingerprint: String,
    /// NodeTypes this table contributed to. A multi-source node
    /// (CRM.Customer + ERP.Customer realising the same Concept)
    /// surfaces an entry for each contributing source × table pair.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub contributed_node_ids: Vec<NodeTypeId>,
    /// EdgeTypes this table contributed to. Bridge / federated link
    /// mappings whose `source_endpoint` or `target_endpoint`
    /// references this table count.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub contributed_edge_ids: Vec<EdgeTypeId>,
    pub status: TableInventoryStatus,
    /// `Imported` rows: when the kernel last read the table.
    /// `AvailableButNotImported`: when the source surface listed it.
    /// `Retracted`: when the operator dropped it.
    pub recorded_at: DateTime<Utc>,
}

impl TableInventoryEntry {
    /// Construct an `Imported` row stamped with `now()` — the
    /// canonical case used by the introspection pipeline.
    pub fn imported(
        source_id: SourceId,
        table_name: impl Into<String>,
        schema_fingerprint: impl Into<String>,
        contributed_node_ids: Vec<NodeTypeId>,
    ) -> Self {
        Self {
            source_id,
            table_name: table_name.into(),
            schema_fingerprint: schema_fingerprint.into(),
            contributed_node_ids,
            contributed_edge_ids: Vec::new(),
            status: TableInventoryStatus::Imported,
            recorded_at: Utc::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imported_helper_stamps_status_and_now() {
        let before = Utc::now();
        let entry = TableInventoryEntry::imported(
            SourceId::new("pg-main"),
            "users",
            "fp-1",
            vec![NodeTypeId::new("nt-user")],
        );
        let after = Utc::now();
        assert_eq!(entry.status, TableInventoryStatus::Imported);
        assert!(entry.recorded_at >= before && entry.recorded_at <= after);
        assert_eq!(entry.contributed_node_ids.len(), 1);
        assert!(entry.contributed_edge_ids.is_empty());
    }

    #[test]
    fn entry_round_trips_through_serde() {
        let entry = TableInventoryEntry::imported(
            SourceId::new("pg-main"),
            "users",
            "fp-1",
            vec![NodeTypeId::new("nt-user")],
        );
        let json = serde_json::to_string(&entry).unwrap();
        let back: TableInventoryEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(back, entry);
    }

    #[test]
    fn status_retracted_round_trips() {
        let mut entry = TableInventoryEntry::imported(
            SourceId::new("pg-main"),
            "audit_log",
            "fp-2",
            Vec::new(),
        );
        entry.status = TableInventoryStatus::Retracted;
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("\"status\":\"retracted\""));
    }
}
