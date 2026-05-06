//! Per-cluster checkpoints for the streaming design path.
//!
//! `design_ontology_batch` runs the LLM design call N times across
//! N clusters per design pass. A transient failure on cluster K
//! previously discarded clusters 0..K's output; this store caches
//! each completed cluster's `InputOntologyDef` keyed by a
//! deterministic `(workspace_id, ontology_draft_id, source_id, signature)`
//! natural key. Replay on retry skips the LLM call when the
//! signature matches.

use async_trait::async_trait;
use uuid::Uuid;

use ox_core::error::OxResult;

#[async_trait]
pub trait DraftClusterCheckpointStore: Send + Sync {
    /// Insert or replace one cluster's checkpoint. Replaces on
    /// `(workspace_id, ontology_draft_id, source_id, signature)` collision —
    /// the cached output is the most recent successful design for
    /// that signature. The store stamps `id` (DB DEFAULT) and
    /// `workspace_id` (bound from the active task-local) regardless
    /// of what the caller passes.
    async fn upsert_draft_cluster_checkpoint(
        &self,
        checkpoint: &ox_ontology::cluster_checkpoint::DraftClusterCheckpoint,
    ) -> OxResult<()>;

    /// Look up one checkpoint by natural key. Returns `Ok(None)`
    /// when the cluster has not been designed yet (cache miss → run
    /// the LLM call).
    async fn find_draft_cluster_checkpoint_by_signature(
        &self,
        ontology_draft_id: Uuid,
        source_id: &str,
        signature: &str,
    ) -> OxResult<Option<ox_ontology::cluster_checkpoint::DraftClusterCheckpoint>>;

    /// Every checkpoint for a draft, newest first. Telemetry +
    /// debug surface; the streaming pipeline keys lookups on
    /// signature directly.
    async fn list_draft_cluster_checkpoints_by_project(
        &self,
        ontology_draft_id: Uuid,
    ) -> OxResult<Vec<ox_ontology::cluster_checkpoint::DraftClusterCheckpoint>>;

    /// Drop every row whose `expires_at` is in the past. Run by the
    /// daily cleanup cron under `SYSTEM_BYPASS`. Returns the number
    /// of rows deleted for telemetry.
    async fn sweep_expired_draft_cluster_checkpoints(&self) -> OxResult<u64>;

    /// Drop every checkpoint for a draft — called when the design
    /// completes successfully (the cached entries are no longer
    /// authoritative once the draft rolls forward).
    async fn delete_draft_cluster_checkpoints_by_project(
        &self,
        ontology_draft_id: Uuid,
    ) -> OxResult<u64>;
}
