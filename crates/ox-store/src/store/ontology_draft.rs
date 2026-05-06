use async_trait::async_trait;
use uuid::Uuid;

use ox_core::error::OxResult;

use crate::models::{OntologyDraft, OntologyDraftSummary, OntologySnapshot, OntologySnapshotSummary};

use super::{AnalysisSnapshot, CursorPage, CursorParams, ExtendResult};

#[async_trait]
pub trait OntologyDraftStore: Send + Sync {
    async fn create_ontology_draft(&self, project: &OntologyDraft) -> OxResult<()>;

    async fn get_ontology_draft(&self, id: Uuid) -> OxResult<Option<OntologyDraft>>;

    async fn list_ontology_drafts(
        &self,
        pagination: &CursorParams,
    ) -> OxResult<CursorPage<OntologyDraftSummary>>;

    async fn update_design_options(
        &self,
        id: Uuid,
        options: &serde_json::Value,
        expected_revision: i32,
    ) -> OxResult<()>;

    async fn update_design_result(
        &self,
        id: Uuid,
        ontology: &serde_json::Value,
        quality_report: Option<&serde_json::Value>,
        expected_revision: i32,
    ) -> OxResult<()>;

    /// Update extend result — updates ontology, source mapping, quality report,
    /// and merges source schema/profile from the extension source.
    async fn update_extend_result(
        &self,
        id: Uuid,
        result: &ExtendResult,
        expected_revision: i32,
    ) -> OxResult<()>;

    /// Replace the analysis snapshot (reanalyze). Resets status to "analyzed",
    /// clears ontology/quality_report, and updates design_options (pruned by caller).
    async fn replace_analysis_snapshot(
        &self,
        id: Uuid,
        snapshot: &AnalysisSnapshot,
        expected_revision: i32,
    ) -> OxResult<()>;

    /// Pin the draft's `parent_version_id` to a fresh canonical
    /// head — branching rebase. CAS-guarded on `expected_revision`
    /// so a concurrent edit to the same draft (analysis-scope
    /// update, design-options change, completion) cannot silently
    /// have its `revision` clobbered by the rebase write. The
    /// caller observes a `Conflict` and refetches when the draft
    /// has moved underneath.
    async fn update_draft_parent_version(
        &self,
        ontology_draft_id: Uuid,
        head_id: Uuid,
        expected_revision: i32,
    ) -> OxResult<()>;

    /// CAS update of `analysis_scope` only.
    async fn update_analysis_scope(
        &self,
        id: Uuid,
        scope: &serde_json::Value,
        expected_revision: i32,
    ) -> OxResult<()>;

    /// Mark the draft as completed and link it to a committed
    /// ontology identity. The caller performs
    /// [`super::OntologyVersionStore::commit_version`] separately,
    /// then hands the resulting **snapshot id** here. The draft
    /// row pins `committed_version_id` to that snapshot — the
    /// draft's audit trail and the branching tree both follow the
    /// link straight to the exact version this draft produced.
    /// Workspace × ontology = 1:1 already determines the lineage,
    /// so the version axis is the only piece worth recording per
    /// draft.
    ///
    /// Uses optimistic CAS on `revision` — stale submissions fail
    /// rather than clobbering a concurrent update.
    async fn complete_ontology_draft(
        &self,
        ontology_draft_id: Uuid,
        committed_version_id: Uuid,
        expected_revision: i32,
    ) -> OxResult<()>;

    async fn delete_ontology_draft(&self, id: Uuid) -> OxResult<bool>;

    /// Archive WIP drafts that haven't been updated within `max_age_days`.
    /// Returns per-workspace counts so the maintenance loop can record one
    /// audit row per affected workspace.
    async fn archive_stale_ontology_drafts(&self, max_age_days: i64) -> OxResult<Vec<(Uuid, u64)>>;

    /// Permanently delete drafts that have been archived for longer than `max_archive_days`.
    /// Returns per-workspace counts so the maintenance loop can record one
    /// audit row per affected workspace.
    async fn delete_archived_ontology_drafts(&self, max_archive_days: i64) -> OxResult<Vec<(Uuid, u64)>>;

    // --- Ontology Snapshots ---

    /// Create an ontology snapshot for a given draft revision.
    /// Uses ON CONFLICT DO NOTHING for idempotency.
    async fn create_ontology_snapshot(
        &self,
        ontology_draft_id: Uuid,
        revision: i32,
        ontology: &serde_json::Value,
        quality_report: Option<&serde_json::Value>,
    ) -> OxResult<()>;

    /// List ontology snapshots for a draft, ordered by revision DESC.
    /// Returns lightweight summaries with node/edge counts extracted from JSONB.
    async fn list_ontology_snapshots(
        &self,
        ontology_draft_id: Uuid,
    ) -> OxResult<Vec<OntologySnapshotSummary>>;

    /// Get a single ontology snapshot by ontology_draft_id + revision.
    async fn get_ontology_snapshot(
        &self,
        ontology_draft_id: Uuid,
        revision: i32,
    ) -> OxResult<Option<OntologySnapshot>>;
}
