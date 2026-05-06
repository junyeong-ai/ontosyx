//! RAGAS-style metric loop persistence.
//!
//! Persistence for the platform's evaluation surface — runs,
//! cases, and per-axis metrics. The mental model lives in
//! [`crate::evaluation`]; this trait is the single async boundary
//! every consumer (cron sweeps, the FE dashboard's API handler,
//! the agent capture hook) calls through.
//!
//! ## Method shape
//!
//! Every method takes the workspace via the task-local
//! `WORKSPACE_ID` scope rather than an explicit parameter. RLS
//! enforces isolation on every read; the write paths read the
//! task-local on the way in so a caller can never accidentally
//! land a row under a different tenant. The `expected_revision`
//! pattern used by [`super::OntologyDraftStore`] is intentionally
//! absent — evaluation rows are append-only / UPSERT-on-natural-
//! key, never in-place CAS.
//!
//! ## Retention
//!
//! Runs are not auto-archived; the dashboard's "compact older
//! than N days" sweep is a future cron. Until then operators
//! delete by UUID via the admin UI.

use async_trait::async_trait;
use uuid::Uuid;

use ox_core::error::OxResult;

use crate::evaluation::{
    EvaluationCase, EvaluationDataset, EvaluationDatasetItem, EvaluationDatasetSummary,
    EvaluationMetric, EvaluationRun, EvaluationRunStatus, RunComparisonReport, RunSummary,
};

use super::{CursorPage, CursorParams};

#[async_trait]
pub trait EvaluationStore: Send + Sync {
    // --- Datasets ------------------------------------------------------

    /// Insert-or-update a dataset on the `(workspace_id, name)`
    /// natural key. Returns the persisted row so the caller picks
    /// up the server-stamped `created_at` without re-fetching.
    /// Re-importing a dataset under the same name preserves `id`
    /// + `created_at` and updates `description` only — every
    /// downstream FK (runs, items) survives the re-import.
    async fn upsert_evaluation_dataset(
        &self,
        dataset: &EvaluationDataset,
    ) -> OxResult<EvaluationDataset>;

    async fn get_evaluation_dataset(&self, id: Uuid) -> OxResult<Option<EvaluationDataset>>;

    /// List datasets visible to the active workspace, newest-
    /// created first. Cursor-paginated to match the rest of the
    /// admin surface. Returns
    /// [`EvaluationDatasetSummary`] (header + item_count) so
    /// the FE renders the inline "12 items" pill without an
    /// N+1 fetch. The canonical [`EvaluationDataset`] shape is
    /// reachable via [`Self::get_evaluation_dataset`] when only
    /// the header is needed.
    async fn list_evaluation_datasets(
        &self,
        pagination: &CursorParams,
    ) -> OxResult<CursorPage<EvaluationDatasetSummary>>;

    /// Cascade-delete a dataset + every item. Runs that referenced
    /// the dataset stay alive (`evaluation_runs.dataset_id` goes
    /// `SET NULL` so historical comparison rows survive).
    async fn delete_evaluation_dataset(&self, id: Uuid) -> OxResult<bool>;

    /// Insert-or-update one dataset item on the `(dataset_id,
    /// item_key)` natural key. Re-importing replaces in place so a
    /// frozen dataset evolves under stable item ids without
    /// fan-out CRUD.
    async fn upsert_evaluation_dataset_item(
        &self,
        item: &EvaluationDatasetItem,
    ) -> OxResult<EvaluationDatasetItem>;

    /// Bulk variant — replace every item under `dataset_id` with
    /// the supplied set in one transaction. Items present in the
    /// caller's slice but missing from the DB are inserted; items
    /// present in the DB but missing from the slice are deleted;
    /// items in both are upserted by `(dataset_id, item_key)`.
    /// Atomic: a single failed item rolls back the whole import,
    /// matching the Phoenix / Braintrust dataset import contract.
    async fn replace_evaluation_dataset_items(
        &self,
        dataset_id: Uuid,
        items: &[EvaluationDatasetItem],
    ) -> OxResult<u64>;

    /// List every item under `dataset_id`, ordered by `item_key`
    /// ASC so the dataset detail panel renders deterministically.
    async fn list_evaluation_dataset_items(
        &self,
        dataset_id: Uuid,
    ) -> OxResult<Vec<EvaluationDatasetItem>>;

    /// Materialise a fresh run from a dataset — copies every item
    /// into `evaluation_cases` keyed on the dataset's `item_key`,
    /// pins the run's `dataset_id` for lineage. Returns the
    /// created run + the case count for the caller's response
    /// envelope. The run starts in `Running` state; the caller
    /// then drives execute / judge as usual.
    ///
    /// Atomic: dataset read + run insert + case bulk-insert all
    /// land in one transaction so partial materialisation is
    /// impossible.
    async fn create_run_from_dataset(
        &self,
        dataset_id: Uuid,
        run_name: &str,
        run_description: &str,
        ontology_version_id: Option<Uuid>,
        run_metadata: serde_json::Value,
    ) -> OxResult<(EvaluationRun, u64)>;

    // --- Runs ----------------------------------------------------------

    /// Insert a new evaluation run in the `Running` state. Returns
    /// the persisted row so the caller observes the server-assigned
    /// `started_at` without a follow-up read.
    async fn create_evaluation_run(&self, run: &EvaluationRun) -> OxResult<EvaluationRun>;

    /// Fetch a single run by primary key. RLS-scoped — returns
    /// `None` for ids outside the active workspace.
    async fn get_evaluation_run(&self, id: Uuid) -> OxResult<Option<EvaluationRun>>;

    /// Conditional lookup by `(workspace_id, name)`. Used by the
    /// online-sampling middleware to find the workspace's
    /// `live_chat_samples` run without listing the entire table.
    /// Returns the most-recent match (`started_at DESC`) when
    /// duplicates exist — the storage layer doesn't enforce a
    /// uniqueness constraint on `name` because operator-driven
    /// runs share the namespace and the same display name is
    /// allowed across cohorts.
    async fn find_evaluation_run_by_name(
        &self,
        name: &str,
    ) -> OxResult<Option<EvaluationRun>>;

    /// List runs in the active workspace, newest-started first.
    async fn list_evaluation_runs(
        &self,
        pagination: &CursorParams,
    ) -> OxResult<CursorPage<EvaluationRun>>;

    /// Transition a run to a terminal status (`Succeeded` /
    /// `Failed` / `Cancelled`) and stamp `completed_at = now()`.
    /// Domain verb because the audit fan-out lives here:
    /// downstream dashboards key on `completed_at` to dim
    /// in-flight rows, and a future cron averages metrics per
    /// completed run.
    async fn complete_evaluation_run(
        &self,
        id: Uuid,
        status: EvaluationRunStatus,
    ) -> OxResult<EvaluationRun>;

    /// Delete a run and cascade its cases + metrics. Returns
    /// `Ok(false)` when no row matched — distinguishing
    /// "deleted" from "not found" without a separate exists
    /// probe.
    async fn delete_evaluation_run(&self, id: Uuid) -> OxResult<bool>;

    /// Run summary — case counts + per-axis aggregate (mean,
    /// count) in one round trip. Drives the run-list "5 of 12
    /// judged · faithfulness 0.78" badge so the operator
    /// triages without drilling into each detail page.
    ///
    /// `total_cases` counts every case attached to the run;
    /// `judged_cases` counts cases that have at least one
    /// metric tagged `metadata.kind = 'judge'` (RAGAS);
    /// `failed_cases` counts cases with `error IS NOT NULL`
    /// (case-execute failed). `axis_means` carries one entry
    /// per metric axis present on the run, the count of cases
    /// scored on that axis, and the mean score across them.
    /// Sorted by `axis ASC` for deterministic FE rendering.
    async fn evaluation_run_summary(&self, run_id: Uuid) -> OxResult<RunSummary>;

    /// Diff two runs over the same dataset. Returns per-case
    /// `(case_key, axis, baseline_score, candidate_score, delta)`
    /// rows + per-axis aggregate summary (mean delta, win-rate,
    /// Cohen's d).
    ///
    /// Both runs MUST share the same `dataset_id`. Diff between
    /// runs over different datasets is not statistically
    /// meaningful (the case_key correspondence is the bridge that
    /// pairs metrics); the impl rejects with `OxError::Validation`
    /// when the lineage doesn't match.
    ///
    /// Per-case rows are ordered `(case_key ASC, axis ASC)` for
    /// deterministic FE rendering. Axes are aggregated across
    /// every case both runs scored on the same axis — un-paired
    /// metrics (axis present on one side only) are silently
    /// dropped from `per_axis` rather than skewing the summary.
    async fn compare_evaluation_runs(
        &self,
        baseline_run_id: Uuid,
        candidate_run_id: Uuid,
    ) -> OxResult<RunComparisonReport>;

    // --- Cases ---------------------------------------------------------

    /// Insert-or-update a case on the `(run_id, case_key)` natural
    /// key. Re-running a dataset replaces previous rows on the
    /// same key so the latest result wins; metrics on the prior
    /// case_id stay attached via the FK cascade only when the
    /// case row is hard-deleted, so re-recording keeps the metric
    /// history.
    async fn upsert_evaluation_case(&self, case: &EvaluationCase) -> OxResult<EvaluationCase>;

    async fn get_evaluation_case(&self, id: Uuid) -> OxResult<Option<EvaluationCase>>;

    /// List every case attached to a run, ordered by `case_key`
    /// ASC so the FE renders the dataset in a stable order across
    /// loads.
    async fn list_evaluation_cases(&self, run_id: Uuid) -> OxResult<Vec<EvaluationCase>>;

    /// Cross-workspace scan for cases ready to judge — cases with
    /// `actual` populated but no `evaluation_metrics` row tagged
    /// `metadata.kind = '<metric_kind>'` yet. Drives the async
    /// judge worker; called under SYSTEM_BYPASS so a single
    /// replica can fan out across every workspace's queue.
    ///
    /// `metric_kind` parametrises the existence check so the
    /// worker can drain the RAGAS rubric (`"judge"`) and the
    /// safety rubric (`"safety_judge"`) independently — a case
    /// missing only one rubric isn't re-judged on the other,
    /// keeping the LLM bill bounded.
    ///
    /// Result is bounded by `limit` so a backlog spike doesn't OOM
    /// the worker — it picks the oldest first (`created_at ASC`)
    /// and the next tick handles whatever remains. Skips
    /// `retrieve_anchors` cases (their input shape carries `kind:
    /// "retrieve_anchors"` and they score deterministically at
    /// execute time — judging would noise the IR axes).
    async fn list_unjudged_cases(
        &self,
        metric_kind: &str,
        limit: u32,
    ) -> OxResult<Vec<EvaluationCase>>;

    // --- Metrics -------------------------------------------------------

    /// Insert-or-update a metric on the `(case_id, name)` natural
    /// key. Re-running the judge replaces previous rows; history
    /// goes through `evaluation_metric_revisions` (deferred) when
    /// the rubric tracking matters. Domain verb because the
    /// upsert carries audit semantics — every replacement stamps
    /// a fresh `created_at`.
    async fn upsert_evaluation_metric(
        &self,
        metric: &EvaluationMetric,
    ) -> OxResult<EvaluationMetric>;

    /// List every metric attached to a case, ordered by `name`
    /// ASC so the FE rubric panel renders deterministically.
    async fn list_evaluation_metrics(&self, case_id: Uuid) -> OxResult<Vec<EvaluationMetric>>;
}
