//! Community-summary primitive — Microsoft GraphRAG-style
//! cluster-level retrieval anchor.
//!
//! `CommunitySummary` carries a hierarchical cluster of
//! ontology entities + an LLM-authored prose summary. The
//! GraphRAG retrieval path uses these to surface
//! collective-level context for broad questions ("how does
//! the customer base look") that no single entity-level
//! match can satisfy.
//!
//! Detection (Leiden / Louvain) and LLM summarisation are
//! deferred to a future cron — this module ships the storage
//! primitive so operators can author summaries manually first
//! and the retrieval path can already consume them.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// One community summary row. Hierarchical (`level`), with the
/// member entity composition and a prose summary the retrieval
/// path embeds in the LLM context window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommunitySummary {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub ontology_version_id: Uuid,
    /// Workspace-supplied or detection-generated id. Stable
    /// across re-summarisation under the same id (the UPSERT
    /// natural key is `(ontology_version_id, community_id)`).
    pub community_id: String,
    /// 0 = top-of-tree (broadest), higher = narrower nested.
    /// Microsoft's recursive Leiden produces 3-5 levels
    /// typically; the platform doesn't pin the depth.
    pub level: u32,
    /// Parallel arrays — `member_entity_kinds[i]` +
    /// `member_logical_ids[i]` together identify one member.
    /// Kept parallel rather than `Vec<EntityRef>` so the
    /// Postgres reverse-index (`gin (member_logical_ids)`)
    /// can answer "which communities contain entity X?" via
    /// array containment.
    pub member_entity_kinds: Vec<String>,
    pub member_logical_ids: Vec<String>,
    /// Short headline — a few words operator surfaces in lists
    /// before the full summary expands.
    pub title: String,
    /// LLM-authored prose. Indexed for the retrieval path's
    /// trigram search.
    pub summary: String,
    pub generated_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn community_summary_round_trips_through_serde() {
        let s = CommunitySummary {
            id: Uuid::new_v4(),
            workspace_id: Uuid::new_v4(),
            ontology_version_id: Uuid::new_v4(),
            community_id: "leiden:level-1:cluster-7".into(),
            level: 1,
            member_entity_kinds: vec!["NodeType".into(), "GlossaryTerm".into()],
            member_logical_ids: vec!["nt_customer".into(), "gt_vip".into()],
            title: "Premium customer cluster".into(),
            summary: "Customers with VIP tier ordering high-value premium products."
                .into(),
            generated_at: Utc::now(),
        };
        let v = serde_json::to_value(&s).unwrap();
        // Wire shape pin: parallel arrays, `level` as plain
        // number, member kinds + logical ids align by index.
        assert_eq!(v["community_id"], "leiden:level-1:cluster-7");
        assert_eq!(v["level"], 1);
        assert_eq!(v["member_entity_kinds"][0], "NodeType");
        assert_eq!(v["member_logical_ids"][0], "nt_customer");
        let back: CommunitySummary = serde_json::from_value(v).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn community_summary_member_arrays_stay_parallel() {
        // Construction-side invariant: kinds + logical_ids
        // arrays are aligned by index. No silent length
        // mismatch — caller responsibility, but the doc
        // pins the contract so a future helper / store impl
        // can assert against it.
        let s = CommunitySummary {
            id: Uuid::new_v4(),
            workspace_id: Uuid::new_v4(),
            ontology_version_id: Uuid::new_v4(),
            community_id: "c-1".into(),
            level: 0,
            member_entity_kinds: vec!["A".into(), "B".into(), "C".into()],
            member_logical_ids: vec!["x".into(), "y".into(), "z".into()],
            title: "test".into(),
            summary: "test".into(),
            generated_at: Utc::now(),
        };
        assert_eq!(s.member_entity_kinds.len(), s.member_logical_ids.len());
    }
}
