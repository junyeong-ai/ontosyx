//! Per-cluster checkpoint primitives — ADR-0027.
//!
//! `design_ontology_batch` runs the LLM design call N times across
//! N table clusters per design pass. A transient failure on cluster
//! K previously discarded clusters 0..K's output and forced the
//! caller to start from scratch — the prior LLM spend was wasted.
//!
//! `DraftClusterCheckpoint` is the persistence shape that lets the
//! streaming pass cache one completed cluster output keyed by a
//! deterministic [`ClusterSignature`]. A re-run with identical
//! `(project_id, source_id, cluster signature)` finds the cached
//! row and skips the LLM call; failed runs only retry the
//! uncompleted clusters.
//!
//! Two pieces this module owns:
//!
//! - `ClusterSignature::from_cluster` — content-addressed digest
//!   of the cluster's input shape. Same tables + same FKs +
//!   same prompt-render hash → same signature.
//! - `DraftClusterCheckpoint` — the persisted record. The store
//!   trait that persists / looks it up lives in `ox-store`
//!   (`DraftClusterCheckpointStore` — added in the integration
//!   slice). This module ships the wire-shape so consumers can
//!   serialise / deserialise checkpoints without depending on
//!   the persistence crate.
//!
//! The checkpoint expires when the operator either (a) completes
//! the design (and the project rolls forward), or (b) explicitly
//! resets — `expires_at` lets a cleanup cron drop stale rows.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::input::InputOntologyDef;
use crate::table_clustering::TableCluster;
use ox_core::source_schema::ForeignKeyDef;

/// Stable digest pinning a cluster's input shape. Two clusters
/// hashing to the same signature are treated as the same logical
/// LLM-design unit even when their non-load-bearing fields differ
/// (cluster `id` is excluded — it's an in-pass numbering, not part
/// of identity).
#[derive(
    Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
pub struct ClusterSignature(String);

impl ClusterSignature {
    /// Hash the cluster's table set + internal FKs + cross-cluster
    /// FKs + the prompt render hash that authored it. The
    /// `prompt_render_hash` argument folds in ADR-0029's render
    /// fingerprint so an admin who edited the prompt body without
    /// bumping `prompt_version` causes a cache miss — the prior
    /// cluster output is no longer authoritative under the new
    /// prompt.
    pub fn from_cluster(cluster: &TableCluster, prompt_render_hash: &str) -> Self {
        let mut hasher = Sha256::new();
        // Tables: alphabetical to keep order-independence.
        let mut tables = cluster.tables.clone();
        tables.sort();
        for t in &tables {
            hasher.update(b"t|");
            hasher.update(t.as_bytes());
            hasher.update(b"\n");
        }
        // FKs: canonical tuple, alphabetised.
        for fk in canonicalise_fks(&cluster.internal_fks, "ifk") {
            hasher.update(fk.as_bytes());
        }
        for fk in canonicalise_fks(&cluster.cross_fks, "xfk") {
            hasher.update(fk.as_bytes());
        }
        hasher.update(b"prh|");
        hasher.update(prompt_render_hash.as_bytes());
        Self(format!("{:x}", hasher.finalize()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Reconstruct a signature from a 64-character lowercase hex
    /// digest. The store layer calls this when lifting a persisted
    /// row into the typed shape — the column was written by
    /// [`Self::from_cluster`] so it's already canonical, but we
    /// validate shape on the way out so a hand-edited or corrupted
    /// row surfaces as a typed error rather than a silent
    /// `signature.as_str()` returning garbage to consumers
    /// downstream.
    pub fn from_hex(hex: String) -> Result<Self, ClusterSignatureParseError> {
        if hex.len() != 64 || !hex.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        {
            return Err(ClusterSignatureParseError);
        }
        Ok(Self(hex))
    }
}

/// `ClusterSignature::from_hex` rejects values that don't match the
/// SHA-256 digest shape `Self::from_cluster` produces. Carries no
/// payload — the offending input would only echo bad data to logs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClusterSignatureParseError;

impl std::fmt::Display for ClusterSignatureParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            "ClusterSignature must be a 64-character lowercase SHA-256 hex digest",
        )
    }
}

impl std::error::Error for ClusterSignatureParseError {}

fn canonicalise_fks(fks: &[ForeignKeyDef], tag: &'static str) -> Vec<String> {
    let mut lines: Vec<String> = fks
        .iter()
        .map(|fk| {
            format!(
                "{tag}|{}.{}->{}.{}\n",
                fk.from_table, fk.from_column, fk.to_table, fk.to_column
            )
        })
        .collect();
    lines.sort();
    lines
}

/// One completed cluster's LLM-design output, ready to persist or
/// replay. The `(project_id, source_id, signature)` triple is the
/// natural key the store layer dedups on — the same signature
/// against the same project + source replays from cache.
///
/// `id` and `workspace_id` reflect persistence state: `None` on a
/// freshly-authored checkpoint (the store mints `id` via the
/// column DEFAULT and stamps `workspace_id` from the active
/// task-local on insert), `Some(_)` on a checkpoint read back from
/// the store. Use [`Self::draft`] to author fresh entries.
#[derive(
    Debug, Clone, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
pub struct DraftClusterCheckpoint {
    /// Surrogate key. Set by the persistence layer on insert; absent
    /// on freshly-authored checkpoints.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<Uuid>,
    /// RLS partition. Stamped by the persistence layer from the
    /// bound task-local; absent on freshly-authored checkpoints.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<Uuid>,
    pub project_id: Uuid,
    pub source_id: String,
    pub signature: ClusterSignature,
    /// Cluster id at the time the checkpoint was written. Useful
    /// for telemetry — the runtime may re-cluster on a later pass
    /// and the id will not match, but the signature still does.
    pub cluster_id: usize,
    /// The cluster's design output. Stored as `InputOntologyDef`
    /// (pre-normalize shape) because that's what `design_ontology_batch`
    /// emits — the merge / reconcile step runs once over all
    /// completed cluster outputs.
    pub output: InputOntologyDef,
    pub created_at: DateTime<Utc>,
    /// Cleanup-cron horizon. Past `expires_at`, the cron drops the
    /// row regardless of whether the design has completed. A
    /// 24-hour default lets a session of design retries hit the
    /// cache without keeping checkpoints around forever.
    pub expires_at: DateTime<Utc>,
}

impl DraftClusterCheckpoint {
    /// Author a fresh checkpoint with the persistence-side fields
    /// (`id`, `workspace_id`) left for the store to populate.
    pub fn draft(
        project_id: Uuid,
        source_id: String,
        signature: ClusterSignature,
        cluster_id: usize,
        output: InputOntologyDef,
        ttl: chrono::Duration,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: None,
            workspace_id: None,
            project_id,
            source_id,
            signature,
            cluster_id,
            output,
            created_at: now,
            expires_at: now + ttl,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fk(from_t: &str, to_t: &str) -> ForeignKeyDef {
        ForeignKeyDef {
            from_table: from_t.to_string(),
            from_column: format!("{to_t}_id"),
            to_table: to_t.to_string(),
            to_column: "id".to_string(),
            inferred: false,
        }
    }

    fn cluster(tables: &[&str], internal: Vec<ForeignKeyDef>) -> TableCluster {
        TableCluster {
            id: 0,
            tables: tables.iter().map(|s| (*s).to_string()).collect(),
            internal_fks: internal,
            cross_fks: Vec::new(),
        }
    }

    #[test]
    fn signature_is_deterministic_for_same_input() {
        let c = cluster(&["users", "orders"], vec![fk("orders", "users")]);
        let a = ClusterSignature::from_cluster(&c, "rh-1");
        let b = ClusterSignature::from_cluster(&c, "rh-1");
        assert_eq!(a, b);
        assert_eq!(a.as_str().len(), 64);
    }

    #[test]
    fn signature_is_table_order_independent() {
        let a = cluster(&["users", "orders"], Vec::new());
        let b = cluster(&["orders", "users"], Vec::new());
        assert_eq!(
            ClusterSignature::from_cluster(&a, "rh-1"),
            ClusterSignature::from_cluster(&b, "rh-1"),
        );
    }

    #[test]
    fn signature_changes_when_table_set_changes() {
        let a = cluster(&["users", "orders"], Vec::new());
        let b = cluster(&["users", "orders", "products"], Vec::new());
        assert_ne!(
            ClusterSignature::from_cluster(&a, "rh-1"),
            ClusterSignature::from_cluster(&b, "rh-1"),
        );
    }

    #[test]
    fn signature_changes_when_fk_set_changes() {
        let a = cluster(&["users", "orders"], Vec::new());
        let b = cluster(&["users", "orders"], vec![fk("orders", "users")]);
        assert_ne!(
            ClusterSignature::from_cluster(&a, "rh-1"),
            ClusterSignature::from_cluster(&b, "rh-1"),
        );
    }

    #[test]
    fn signature_changes_when_prompt_render_hash_changes() {
        // ADR-0027 + ADR-0029 interaction: an admin who edits the
        // prompt body without bumping `prompt_version` shifts the
        // render hash, which shifts the signature, which forces a
        // cache miss — the cached cluster output is no longer
        // authoritative under the new prompt.
        let c = cluster(&["users", "orders"], Vec::new());
        let a = ClusterSignature::from_cluster(&c, "rh-old");
        let b = ClusterSignature::from_cluster(&c, "rh-new");
        assert_ne!(a, b);
    }

    #[test]
    fn signature_does_not_depend_on_cluster_id() {
        // `id` is in-pass numbering — a re-cluster may renumber the
        // same input. The signature must not move.
        let mut a = cluster(&["users"], Vec::new());
        a.id = 0;
        let mut b = cluster(&["users"], Vec::new());
        b.id = 7;
        assert_eq!(
            ClusterSignature::from_cluster(&a, "rh-1"),
            ClusterSignature::from_cluster(&b, "rh-1"),
        );
    }

    #[test]
    fn signature_round_trips_through_serde() {
        let c = cluster(&["users"], Vec::new());
        let s = ClusterSignature::from_cluster(&c, "rh-1");
        let json = serde_json::to_string(&s).unwrap();
        let back: ClusterSignature = serde_json::from_str(&json).unwrap();
        assert_eq!(back, s);
    }
}
