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

use crate::evaluation::{EvaluationCase, EvaluationMetric, EvaluationRun, EvaluationRunStatus};

use super::{CursorPage, CursorParams};

#[async_trait]
pub trait EvaluationStore: Send + Sync {
    // --- Runs ----------------------------------------------------------

    /// Insert a new evaluation run in the `Running` state. Returns
    /// the persisted row so the caller observes the server-assigned
    /// `started_at` without a follow-up read.
    async fn create_evaluation_run(&self, run: &EvaluationRun) -> OxResult<EvaluationRun>;

    /// Fetch a single run by primary key. RLS-scoped — returns
    /// `None` for ids outside the active workspace.
    async fn get_evaluation_run(&self, id: Uuid) -> OxResult<Option<EvaluationRun>>;

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
