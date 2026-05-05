//! [`EvaluationStore`] PG impl — runs / cases / metrics.
//!
//! Workspace isolation rides RLS — every read/write below carries
//! the `super::require_workspace_context()?` guard so a missing
//! `WORKSPACE_ID` task-local fails loudly instead of silently
//! crossing tenants. Writes additionally bind the task-local
//! workspace into the row directly so a caller cannot smuggle a
//! row under a different tenant via the input struct.
//!
//! Natural-key UPSERTs:
//!
//! - `(run_id, case_key)` → cases
//! - `(case_id, name)`     → metrics
//!
//! Both replace-on-conflict so re-running the evaluator is
//! idempotent: the latest result wins, prior rows on the same
//! natural key are overwritten, and metrics survive the case-row
//! replacement only when the case is hard-deleted (FK `ON DELETE
//! CASCADE`). The two surfaces share this contract; the store
//! trait pins it in doc.

use crate::evaluation::{
    parse_run_status, EvaluationCapture, EvaluationCase, EvaluationContext, EvaluationMetric,
    EvaluationRun, EvaluationRunStatus,
};
use crate::store::EvaluationStore;

use super::*;

/// Crate-private row mirror for `evaluation_runs`. Keeps the
/// `status` decode in one place.
#[derive(sqlx::FromRow)]
struct EvaluationRunRow {
    id: Uuid,
    workspace_id: Uuid,
    ontology_version_id: Option<Uuid>,
    name: String,
    description: String,
    status: String,
    started_at: chrono::DateTime<chrono::Utc>,
    completed_at: Option<chrono::DateTime<chrono::Utc>>,
    metadata: serde_json::Value,
}

impl EvaluationRunRow {
    fn into_domain(self) -> OxResult<EvaluationRun> {
        Ok(EvaluationRun {
            id: self.id,
            workspace_id: self.workspace_id,
            ontology_version_id: self.ontology_version_id,
            name: self.name,
            description: self.description,
            status: parse_run_status(&self.status)?,
            started_at: self.started_at,
            completed_at: self.completed_at,
            metadata: self.metadata,
        })
    }
}

#[derive(sqlx::FromRow)]
struct EvaluationCaseRow {
    id: Uuid,
    run_id: Uuid,
    workspace_id: Uuid,
    case_key: String,
    input: serde_json::Value,
    expected: Option<serde_json::Value>,
    actual: Option<serde_json::Value>,
    error: Option<String>,
    latency_ms: Option<i64>,
    created_at: chrono::DateTime<chrono::Utc>,
}

impl From<EvaluationCaseRow> for EvaluationCase {
    fn from(r: EvaluationCaseRow) -> Self {
        Self {
            id: r.id,
            run_id: r.run_id,
            workspace_id: r.workspace_id,
            case_key: r.case_key,
            input: r.input,
            expected: r.expected,
            actual: r.actual,
            error: r.error,
            latency_ms: r.latency_ms,
            created_at: r.created_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct EvaluationMetricRow {
    id: Uuid,
    case_id: Uuid,
    workspace_id: Uuid,
    name: String,
    score: f64,
    reasoning: Option<String>,
    metadata: serde_json::Value,
    created_at: chrono::DateTime<chrono::Utc>,
}

impl From<EvaluationMetricRow> for EvaluationMetric {
    fn from(r: EvaluationMetricRow) -> Self {
        Self {
            id: r.id,
            case_id: r.case_id,
            workspace_id: r.workspace_id,
            name: r.name,
            score: r.score,
            reasoning: r.reasoning,
            metadata: r.metadata,
            created_at: r.created_at,
        }
    }
}

#[async_trait]
impl EvaluationStore for PostgresStore {
    // --- Runs ----------------------------------------------------------

    #[tracing::instrument(level = "debug", skip_all, fields(run.name = %run.name))]
    async fn create_evaluation_run(&self, run: &EvaluationRun) -> OxResult<EvaluationRun> {
        let workspace_id = super::bound_workspace_id_for_dml()?;
        let row: EvaluationRunRow = sqlx::query_as(
            "INSERT INTO evaluation_runs
                (id, workspace_id, ontology_version_id, name, description,
                 status, started_at, completed_at, metadata)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
             RETURNING id, workspace_id, ontology_version_id, name, description,
                       status, started_at, completed_at, metadata",
        )
        .bind(run.id)
        .bind(workspace_id)
        .bind(run.ontology_version_id)
        .bind(&run.name)
        .bind(&run.description)
        .bind(run.status.as_str())
        .bind(run.started_at)
        .bind(run.completed_at)
        .bind(&run.metadata)
        .fetch_one(&self.pool)
        .await
        .map_err(to_ox_error)?;
        row.into_domain()
    }

    #[tracing::instrument(level = "debug", skip_all, fields(run_id = %id))]
    async fn get_evaluation_run(&self, id: Uuid) -> OxResult<Option<EvaluationRun>> {
        super::require_workspace_context()?;
        let row: Option<EvaluationRunRow> = sqlx::query_as(
            "SELECT id, workspace_id, ontology_version_id, name, description,
                    status, started_at, completed_at, metadata
             FROM evaluation_runs WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)?;
        row.map(EvaluationRunRow::into_domain).transpose()
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_evaluation_runs(
        &self,
        pagination: &CursorParams,
    ) -> OxResult<CursorPage<EvaluationRun>> {
        super::require_workspace_context()?;
        let limit = pagination.effective_limit();
        let fetch_limit = limit + 1;

        let mut qb: sqlx::QueryBuilder<sqlx::Postgres> = sqlx::QueryBuilder::new(
            "SELECT id, workspace_id, ontology_version_id, name, description,
                    status, started_at, completed_at, metadata
             FROM evaluation_runs WHERE TRUE",
        );
        if let Some((cursor_ts, cursor_id)) = pagination.cursor_parts() {
            qb.push(" AND (started_at, id) < (");
            qb.push_bind(cursor_ts);
            qb.push(", ");
            qb.push_bind(cursor_id);
            qb.push(")");
        }
        qb.push(" ORDER BY started_at DESC, id DESC LIMIT ");
        qb.push_bind(fetch_limit);

        let rows: Vec<EvaluationRunRow> = qb
            .build_query_as()
            .fetch_all(&self.pool)
            .await
            .map_err(to_ox_error)?;
        let items: Vec<EvaluationRun> = rows
            .into_iter()
            .map(EvaluationRunRow::into_domain)
            .collect::<OxResult<_>>()?;
        Ok(super::build_cursor_page(items, limit, |r| {
            (r.started_at, r.id)
        }))
    }

    #[tracing::instrument(level = "debug", skip_all, fields(run_id = %id, status = %status.as_str()))]
    async fn complete_evaluation_run(
        &self,
        id: Uuid,
        status: EvaluationRunStatus,
    ) -> OxResult<EvaluationRun> {
        super::require_workspace_context()?;
        let row: Option<EvaluationRunRow> = sqlx::query_as(
            "UPDATE evaluation_runs
             SET status = $2, completed_at = now()
             WHERE id = $1
             RETURNING id, workspace_id, ontology_version_id, name, description,
                       status, started_at, completed_at, metadata",
        )
        .bind(id)
        .bind(status.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)?;
        row.ok_or_else(|| OxError::NotFound {
            entity: format!("evaluation_runs id={id}"),
        })?
        .into_domain()
    }

    #[tracing::instrument(level = "debug", skip_all, fields(run_id = %id))]
    async fn delete_evaluation_run(&self, id: Uuid) -> OxResult<bool> {
        super::require_workspace_context()?;
        let result = sqlx::query("DELETE FROM evaluation_runs WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        Ok(result.rows_affected() > 0)
    }

    // --- Cases ---------------------------------------------------------

    #[tracing::instrument(level = "debug", skip_all, fields(run_id = %case.run_id, case_key = %case.case_key))]
    async fn upsert_evaluation_case(&self, case: &EvaluationCase) -> OxResult<EvaluationCase> {
        let workspace_id = super::bound_workspace_id_for_dml()?;
        let row: EvaluationCaseRow = sqlx::query_as(
            "INSERT INTO evaluation_cases
                (id, run_id, workspace_id, case_key, input, expected, actual,
                 error, latency_ms, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
             ON CONFLICT (run_id, case_key) DO UPDATE SET
                input = EXCLUDED.input,
                expected = EXCLUDED.expected,
                actual = EXCLUDED.actual,
                error = EXCLUDED.error,
                latency_ms = EXCLUDED.latency_ms
             RETURNING id, run_id, workspace_id, case_key, input, expected,
                       actual, error, latency_ms, created_at",
        )
        .bind(case.id)
        .bind(case.run_id)
        .bind(workspace_id)
        .bind(&case.case_key)
        .bind(&case.input)
        .bind(&case.expected)
        .bind(&case.actual)
        .bind(&case.error)
        .bind(case.latency_ms)
        .bind(case.created_at)
        .fetch_one(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(row.into())
    }

    #[tracing::instrument(level = "debug", skip_all, fields(case_id = %id))]
    async fn get_evaluation_case(&self, id: Uuid) -> OxResult<Option<EvaluationCase>> {
        super::require_workspace_context()?;
        let row: Option<EvaluationCaseRow> = sqlx::query_as(
            "SELECT id, run_id, workspace_id, case_key, input, expected, actual,
                    error, latency_ms, created_at
             FROM evaluation_cases WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(row.map(EvaluationCase::from))
    }

    #[tracing::instrument(level = "debug", skip_all, fields(run_id = %run_id))]
    async fn list_evaluation_cases(&self, run_id: Uuid) -> OxResult<Vec<EvaluationCase>> {
        super::require_workspace_context()?;
        let rows: Vec<EvaluationCaseRow> = sqlx::query_as(
            "SELECT id, run_id, workspace_id, case_key, input, expected, actual,
                    error, latency_ms, created_at
             FROM evaluation_cases
             WHERE run_id = $1
             ORDER BY case_key ASC",
        )
        .bind(run_id)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(rows.into_iter().map(EvaluationCase::from).collect())
    }

    // --- Metrics -------------------------------------------------------

    #[tracing::instrument(level = "debug", skip_all, fields(case_id = %metric.case_id, metric.name = %metric.name))]
    async fn record_evaluation_metric(
        &self,
        metric: &EvaluationMetric,
    ) -> OxResult<EvaluationMetric> {
        let workspace_id = super::bound_workspace_id_for_dml()?;
        let row: EvaluationMetricRow = sqlx::query_as(
            "INSERT INTO evaluation_metrics
                (id, case_id, workspace_id, name, score, reasoning, metadata,
                 created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
             ON CONFLICT (case_id, name) DO UPDATE SET
                score = EXCLUDED.score,
                reasoning = EXCLUDED.reasoning,
                metadata = EXCLUDED.metadata,
                created_at = EXCLUDED.created_at
             RETURNING id, case_id, workspace_id, name, score, reasoning,
                       metadata, created_at",
        )
        .bind(metric.id)
        .bind(metric.case_id)
        .bind(workspace_id)
        .bind(&metric.name)
        .bind(metric.score)
        .bind(&metric.reasoning)
        .bind(&metric.metadata)
        .bind(metric.created_at)
        .fetch_one(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(row.into())
    }

    #[tracing::instrument(level = "debug", skip_all, fields(case_id = %case_id))]
    async fn list_evaluation_metrics(
        &self,
        case_id: Uuid,
    ) -> OxResult<Vec<EvaluationMetric>> {
        super::require_workspace_context()?;
        let rows: Vec<EvaluationMetricRow> = sqlx::query_as(
            "SELECT id, case_id, workspace_id, name, score, reasoning, metadata,
                    created_at
             FROM evaluation_metrics
             WHERE case_id = $1
             ORDER BY name ASC",
        )
        .bind(case_id)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(rows.into_iter().map(EvaluationMetric::from).collect())
    }
}

/// Storage-backed [`EvaluationCapture`]. Routes every latency
/// observation to a fresh row on `evaluation_metrics` with the
/// operation name as the rubric axis.
///
/// The capture is workspace-scoped via the same task-local
/// guard the rest of the store uses; an evaluation scope
/// without a workspace context fails the underlying
/// `record_evaluation_metric` call rather than silently
/// landing rows under a different tenant.
#[async_trait]
impl EvaluationCapture for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all, fields(case_id = %ctx.case_id, operation = %operation, latency_ms = latency_ms))]
    async fn record_latency(
        &self,
        ctx: &EvaluationContext,
        operation: &str,
        latency_ms: i64,
    ) -> OxResult<()> {
        let workspace_id = super::bound_workspace_id_for_dml()?;
        let metric = EvaluationMetric {
            id: Uuid::now_v7(),
            case_id: ctx.case_id,
            workspace_id,
            name: format!("latency_ms.{operation}"),
            score: latency_ms as f64,
            reasoning: None,
            metadata: serde_json::json!({
                "kind": "latency_ms",
                "operation": operation,
                "run_id": ctx.run_id,
                "case_key": ctx.case_key,
            }),
            created_at: chrono::Utc::now(),
        };
        self.record_evaluation_metric(&metric).await.map(|_| ())
    }
}
