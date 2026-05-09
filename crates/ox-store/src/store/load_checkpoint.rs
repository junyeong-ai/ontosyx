//! Watermark-based incremental load state.

use async_trait::async_trait;
use uuid::Uuid;

use ox_core::error::OxResult;

use crate::models::LoadCheckpoint;

#[async_trait]
pub trait LoadCheckpointStore: Send + Sync {
    /// Get the latest checkpoint for a specific (draft, source_table, graph_label) combination.
    async fn get_load_checkpoint(
        &self,
        ontology_draft_id: Uuid,
        source_table: &str,
        graph_label: &str,
    ) -> OxResult<Option<LoadCheckpoint>>;

    /// Create or update a checkpoint (matched by draft + source_table + graph_label).
    async fn upsert_load_checkpoint(&self, checkpoint: &LoadCheckpoint) -> OxResult<()>;

    /// List all checkpoints for an ontology draft.
    async fn list_load_checkpoints(&self, ontology_draft_id: Uuid)
    -> OxResult<Vec<LoadCheckpoint>>;

    /// Delete a specific checkpoint (forces a full reload on next run).
    async fn delete_load_checkpoint(&self, id: Uuid) -> OxResult<bool>;
}
