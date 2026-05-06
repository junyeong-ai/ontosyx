//! Data-load provenance tracking.

use async_trait::async_trait;
use uuid::Uuid;

use ox_core::error::OxResult;

use crate::models::{LineageEntry, LineageSummary};

#[async_trait]
pub trait LineageStore: Send + Sync {
    /// Record the start of a data load operation.
    async fn create_lineage_entry(&self, entry: &LineageEntry) -> OxResult<()>;

    /// Mark a lineage entry as completed (success or failure).
    async fn complete_lineage_entry(
        &self,
        id: Uuid,
        record_count: i64,
        status: &str,
        error_message: Option<&str>,
    ) -> OxResult<()>;

    /// Get lineage entries for a specific graph label.
    async fn list_lineage_for_label(&self, graph_label: &str) -> OxResult<Vec<LineageEntry>>;

    /// Get lineage entries for an ontology draft.
    async fn list_lineage_for_ontology_draft(
        &self,
        ontology_draft_id: Uuid,
    ) -> OxResult<Vec<LineageEntry>>;

    /// Get a summary of lineage per graph label (for overview).
    async fn lineage_summary(&self) -> OxResult<Vec<LineageSummary>>;
}
