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
    parse_run_status, EvaluationCapture, EvaluationCase, EvaluationContext, EvaluationDataset,
    EvaluationDatasetItem, EvaluationMetric, EvaluationRun, EvaluationRunStatus,
};
use crate::store::{CursorPage, CursorParams, EvaluationStore};

use super::*;

/// Crate-private row mirror for `evaluation_runs`. Keeps the
/// `status` decode in one place.
#[derive(sqlx::FromRow)]
struct EvaluationRunRow {
    id: Uuid,
    workspace_id: Uuid,
    ontology_version_id: Option<Uuid>,
    dataset_id: Option<Uuid>,
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
            dataset_id: self.dataset_id,
            name: self.name,
            description: self.description,
            status: parse_run_status(&self.status)?,
            started_at: self.started_at,
            completed_at: self.completed_at,
            metadata: self.metadata,
        })
    }
}

/// Row mirror for `evaluation_datasets`.
#[derive(sqlx::FromRow)]
struct EvaluationDatasetRow {
    id: Uuid,
    workspace_id: Uuid,
    name: String,
    description: String,
    created_at: chrono::DateTime<chrono::Utc>,
}

impl From<EvaluationDatasetRow> for EvaluationDataset {
    fn from(r: EvaluationDatasetRow) -> Self {
        Self {
            id: r.id,
            workspace_id: r.workspace_id,
            name: r.name,
            description: r.description,
            created_at: r.created_at,
        }
    }
}

/// Row mirror for `evaluation_dataset_items`.
#[derive(sqlx::FromRow)]
struct EvaluationDatasetItemRow {
    id: Uuid,
    dataset_id: Uuid,
    workspace_id: Uuid,
    item_key: String,
    input: serde_json::Value,
    expected: Option<serde_json::Value>,
    metadata: serde_json::Value,
    created_at: chrono::DateTime<chrono::Utc>,
}

impl From<EvaluationDatasetItemRow> for EvaluationDatasetItem {
    fn from(r: EvaluationDatasetItemRow) -> Self {
        Self {
            id: r.id,
            dataset_id: r.dataset_id,
            workspace_id: r.workspace_id,
            item_key: r.item_key,
            input: r.input,
            expected: r.expected,
            metadata: r.metadata,
            created_at: r.created_at,
        }
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
    metadata: serde_json::Value,
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
            metadata: r.metadata,
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
    // --- Datasets ------------------------------------------------------

    #[tracing::instrument(level = "debug", skip_all, fields(dataset.name = %dataset.name))]
    async fn upsert_evaluation_dataset(
        &self,
        dataset: &EvaluationDataset,
    ) -> OxResult<EvaluationDataset> {
        let workspace_id = super::bound_workspace_id_for_dml()?;
        let row: EvaluationDatasetRow = sqlx::query_as(
            "INSERT INTO evaluation_datasets
                (id, workspace_id, name, description, created_at)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (workspace_id, name) DO UPDATE SET
                description = EXCLUDED.description
             RETURNING id, workspace_id, name, description, created_at",
        )
        .bind(dataset.id)
        .bind(workspace_id)
        .bind(&dataset.name)
        .bind(&dataset.description)
        .bind(dataset.created_at)
        .fetch_one(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(row.into())
    }

    #[tracing::instrument(level = "debug", skip_all, fields(dataset_id = %id))]
    async fn get_evaluation_dataset(
        &self,
        id: Uuid,
    ) -> OxResult<Option<EvaluationDataset>> {
        super::require_workspace_context()?;
        let row: Option<EvaluationDatasetRow> = sqlx::query_as(
            "SELECT id, workspace_id, name, description, created_at
             FROM evaluation_datasets WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(row.map(EvaluationDataset::from))
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_evaluation_datasets(
        &self,
        pagination: &CursorParams,
    ) -> OxResult<CursorPage<EvaluationDataset>> {
        super::require_workspace_context()?;
        let limit = pagination.effective_limit();
        let fetch_limit = limit + 1;

        let mut qb: sqlx::QueryBuilder<sqlx::Postgres> = sqlx::QueryBuilder::new(
            "SELECT id, workspace_id, name, description, created_at
             FROM evaluation_datasets WHERE TRUE",
        );
        if let Some((cursor_ts, cursor_id)) = pagination.cursor_parts() {
            qb.push(" AND (created_at, id) < (");
            qb.push_bind(cursor_ts);
            qb.push(", ");
            qb.push_bind(cursor_id);
            qb.push(")");
        }
        qb.push(" ORDER BY created_at DESC, id DESC LIMIT ");
        qb.push_bind(fetch_limit);

        let rows: Vec<EvaluationDatasetRow> = qb
            .build_query_as()
            .fetch_all(&self.pool)
            .await
            .map_err(to_ox_error)?;
        let items: Vec<EvaluationDataset> =
            rows.into_iter().map(EvaluationDataset::from).collect();
        Ok(super::build_cursor_page(items, limit, |d| {
            (d.created_at, d.id)
        }))
    }

    #[tracing::instrument(level = "debug", skip_all, fields(dataset_id = %id))]
    async fn delete_evaluation_dataset(&self, id: Uuid) -> OxResult<bool> {
        super::require_workspace_context()?;
        let result = sqlx::query("DELETE FROM evaluation_datasets WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        Ok(result.rows_affected() > 0)
    }

    #[tracing::instrument(level = "debug", skip_all, fields(
        dataset_id = %item.dataset_id,
        item_key = %item.item_key,
    ))]
    async fn upsert_evaluation_dataset_item(
        &self,
        item: &EvaluationDatasetItem,
    ) -> OxResult<EvaluationDatasetItem> {
        let workspace_id = super::bound_workspace_id_for_dml()?;
        let row: EvaluationDatasetItemRow = sqlx::query_as(
            "INSERT INTO evaluation_dataset_items
                (id, dataset_id, workspace_id, item_key, input, expected, metadata,
                 created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
             ON CONFLICT (dataset_id, item_key) DO UPDATE SET
                input = EXCLUDED.input,
                expected = EXCLUDED.expected,
                metadata = EXCLUDED.metadata
             RETURNING id, dataset_id, workspace_id, item_key, input, expected,
                       metadata, created_at",
        )
        .bind(item.id)
        .bind(item.dataset_id)
        .bind(workspace_id)
        .bind(&item.item_key)
        .bind(&item.input)
        .bind(&item.expected)
        .bind(&item.metadata)
        .bind(item.created_at)
        .fetch_one(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(row.into())
    }

    #[tracing::instrument(level = "debug", skip_all, fields(
        dataset_id = %dataset_id,
        item_count = items.len(),
    ))]
    async fn replace_evaluation_dataset_items(
        &self,
        dataset_id: Uuid,
        items: &[EvaluationDatasetItem],
    ) -> OxResult<u64> {
        let workspace_id = super::bound_workspace_id_for_dml()?;
        let mut tx = self.pool.begin().await.map_err(to_ox_error)?;

        // Drop items not in the supplied set. The keep-list ride
        // on the natural key so a re-import that renames an
        // `item_key` correctly removes the old row + inserts the
        // new one rather than leaking the stale entry.
        let keep_keys: Vec<&str> = items.iter().map(|i| i.item_key.as_str()).collect();
        sqlx::query(
            "DELETE FROM evaluation_dataset_items
             WHERE dataset_id = $1 AND NOT (item_key = ANY($2))",
        )
        .bind(dataset_id)
        .bind(&keep_keys)
        .execute(&mut *tx)
        .await
        .map_err(to_ox_error)?;

        // UPSERT every supplied item.
        for item in items {
            sqlx::query(
                "INSERT INTO evaluation_dataset_items
                    (id, dataset_id, workspace_id, item_key, input, expected, metadata,
                     created_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                 ON CONFLICT (dataset_id, item_key) DO UPDATE SET
                    input = EXCLUDED.input,
                    expected = EXCLUDED.expected,
                    metadata = EXCLUDED.metadata",
            )
            .bind(item.id)
            .bind(dataset_id)
            .bind(workspace_id)
            .bind(&item.item_key)
            .bind(&item.input)
            .bind(&item.expected)
            .bind(&item.metadata)
            .bind(item.created_at)
            .execute(&mut *tx)
            .await
            .map_err(to_ox_error)?;
        }

        tx.commit().await.map_err(to_ox_error)?;
        Ok(items.len() as u64)
    }

    #[tracing::instrument(level = "debug", skip_all, fields(dataset_id = %dataset_id))]
    async fn list_evaluation_dataset_items(
        &self,
        dataset_id: Uuid,
    ) -> OxResult<Vec<EvaluationDatasetItem>> {
        super::require_workspace_context()?;
        let rows: Vec<EvaluationDatasetItemRow> = sqlx::query_as(
            "SELECT id, dataset_id, workspace_id, item_key, input, expected, metadata,
                    created_at
             FROM evaluation_dataset_items
             WHERE dataset_id = $1
             ORDER BY item_key ASC",
        )
        .bind(dataset_id)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(rows.into_iter().map(EvaluationDatasetItem::from).collect())
    }

    #[tracing::instrument(level = "debug", skip_all, fields(dataset_id = %dataset_id, run.name = %run_name))]
    async fn create_run_from_dataset(
        &self,
        dataset_id: Uuid,
        run_name: &str,
        run_description: &str,
        ontology_version_id: Option<Uuid>,
        run_metadata: serde_json::Value,
    ) -> OxResult<(EvaluationRun, u64)> {
        let workspace_id = super::bound_workspace_id_for_dml()?;
        let mut tx = self.pool.begin().await.map_err(to_ox_error)?;

        // Verify the dataset exists in the active workspace
        // before consuming it. RLS already filters cross-tenant
        // ids, but a typed `NotFound` reads cleaner than a
        // confusing "0 items" run.
        let exists: Option<(Uuid,)> = sqlx::query_as(
            "SELECT id FROM evaluation_datasets WHERE id = $1",
        )
        .bind(dataset_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(to_ox_error)?;
        if exists.is_none() {
            return Err(OxError::NotFound {
                entity: format!("evaluation_datasets id={dataset_id}"),
            });
        }

        let run_id = Uuid::now_v7();
        let started_at = chrono::Utc::now();
        let run_row: EvaluationRunRow = sqlx::query_as(
            "INSERT INTO evaluation_runs
                (id, workspace_id, ontology_version_id, dataset_id, name, description,
                 status, started_at, completed_at, metadata)
             VALUES ($1, $2, $3, $4, $5, $6, 'running', $7, NULL, $8)
             RETURNING id, workspace_id, ontology_version_id, dataset_id, name, description,
                       status, started_at, completed_at, metadata",
        )
        .bind(run_id)
        .bind(workspace_id)
        .bind(ontology_version_id)
        .bind(dataset_id)
        .bind(run_name)
        .bind(run_description)
        .bind(started_at)
        .bind(&run_metadata)
        .fetch_one(&mut *tx)
        .await
        .map_err(to_ox_error)?;

        // Materialise dataset items into cases. Single round-trip
        // via `INSERT … SELECT` so a 10k-item dataset doesn't
        // generate 10k network hops.
        let case_count: (i64,) = sqlx::query_as(
            "WITH inserted AS (
                INSERT INTO evaluation_cases
                    (id, run_id, workspace_id, case_key, input, expected, actual,
                     error, latency_ms, metadata, created_at)
                SELECT
                    gen_random_uuid(), $2, $3, item.item_key, item.input,
                    item.expected, NULL, NULL, NULL,
                    '{}'::jsonb, now()
                FROM evaluation_dataset_items item
                WHERE item.dataset_id = $1
                RETURNING 1
             )
             SELECT COUNT(*) FROM inserted",
        )
        .bind(dataset_id)
        .bind(run_id)
        .bind(workspace_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(to_ox_error)?;

        tx.commit().await.map_err(to_ox_error)?;
        Ok((run_row.into_domain()?, case_count.0 as u64))
    }

    // --- Runs ----------------------------------------------------------

    #[tracing::instrument(level = "debug", skip_all, fields(run.name = %run.name))]
    async fn create_evaluation_run(&self, run: &EvaluationRun) -> OxResult<EvaluationRun> {
        let workspace_id = super::bound_workspace_id_for_dml()?;
        let row: EvaluationRunRow = sqlx::query_as(
            "INSERT INTO evaluation_runs
                (id, workspace_id, ontology_version_id, dataset_id, name, description,
                 status, started_at, completed_at, metadata)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
             RETURNING id, workspace_id, ontology_version_id, dataset_id, name, description,
                       status, started_at, completed_at, metadata",
        )
        .bind(run.id)
        .bind(workspace_id)
        .bind(run.ontology_version_id)
        .bind(run.dataset_id)
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
            "SELECT id, workspace_id, ontology_version_id, dataset_id, name, description,
                    status, started_at, completed_at, metadata
             FROM evaluation_runs WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)?;
        row.map(EvaluationRunRow::into_domain).transpose()
    }

    #[tracing::instrument(level = "debug", skip_all, fields(name = %name))]
    async fn find_evaluation_run_by_name(
        &self,
        name: &str,
    ) -> OxResult<Option<EvaluationRun>> {
        super::require_workspace_context()?;
        let row: Option<EvaluationRunRow> = sqlx::query_as(
            "SELECT id, workspace_id, ontology_version_id, dataset_id, name, description,
                    status, started_at, completed_at, metadata
             FROM evaluation_runs
             WHERE name = $1
             ORDER BY started_at DESC
             LIMIT 1",
        )
        .bind(name)
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
            "SELECT id, workspace_id, ontology_version_id, dataset_id, name, description,
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
             RETURNING id, workspace_id, ontology_version_id, dataset_id, name, description,
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

    #[tracing::instrument(level = "debug", skip_all, fields(
        baseline = %baseline_run_id,
        candidate = %candidate_run_id,
    ))]
    async fn compare_evaluation_runs(
        &self,
        baseline_run_id: Uuid,
        candidate_run_id: Uuid,
    ) -> OxResult<crate::evaluation::RunComparisonReport> {
        use crate::evaluation::{RunAxisSummary, RunComparisonReport, RunMetricDelta};
        super::require_workspace_context()?;

        // 1. Verify both runs exist + share dataset_id. RLS
        //    already filters cross-tenant ids; the dataset
        //    correspondence check is the pair gate.
        let pair: Option<(Option<Uuid>, Option<Uuid>)> = sqlx::query_as(
            "SELECT
                (SELECT dataset_id FROM evaluation_runs WHERE id = $1) AS baseline_dataset,
                (SELECT dataset_id FROM evaluation_runs WHERE id = $2) AS candidate_dataset",
        )
        .bind(baseline_run_id)
        .bind(candidate_run_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)?;
        let (baseline_dataset, candidate_dataset) = match pair {
            Some((Some(b), Some(c))) => (b, c),
            _ => {
                return Err(OxError::Validation {
                    field: "run_pair".to_string(),
                    message: format!(
                        "Cannot diff runs: at least one of {baseline_run_id} / {candidate_run_id} \
                         is not associated with a dataset (ad-hoc runs cannot be compared — \
                         only dataset-materialised runs share the case_key correspondence the \
                         diff requires)."
                    ),
                });
            }
        };
        if baseline_dataset != candidate_dataset {
            return Err(OxError::Validation {
                field: "run_pair".to_string(),
                message: format!(
                    "Cannot diff runs over different datasets: baseline={baseline_dataset}, \
                     candidate={candidate_dataset}. Only runs that materialised from the same \
                     dataset share the case_key correspondence the diff requires."
                ),
            });
        }
        let dataset_id = baseline_dataset;

        // 2. Per-(case_key, axis) inner join across both runs.
        //    Metric `(case_id, name)` is unique by the natural-
        //    key UPSERT contract, so each (case_key, axis) pair
        //    yields exactly one row per side. Returned ordering
        //    is stable for FE rendering.
        #[derive(sqlx::FromRow)]
        struct DeltaRow {
            case_key: String,
            axis: String,
            baseline_score: f64,
            candidate_score: f64,
            delta: f64,
        }
        let rows: Vec<DeltaRow> = sqlx::query_as(
            "SELECT
                c1.case_key                            AS case_key,
                m1.name                                AS axis,
                m1.score                               AS baseline_score,
                m2.score                               AS candidate_score,
                (m2.score - m1.score)                  AS delta
            FROM evaluation_cases c1
            JOIN evaluation_cases c2
                ON c2.case_key = c1.case_key AND c2.run_id = $2
            JOIN evaluation_metrics m1
                ON m1.case_id = c1.id
            JOIN evaluation_metrics m2
                ON m2.case_id = c2.id AND m2.name = m1.name
            WHERE c1.run_id = $1
            ORDER BY c1.case_key ASC, m1.name ASC",
        )
        .bind(baseline_run_id)
        .bind(candidate_run_id)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)?;

        let per_case: Vec<RunMetricDelta> = rows
            .iter()
            .map(|r| RunMetricDelta {
                case_key: r.case_key.clone(),
                axis: r.axis.clone(),
                baseline_score: r.baseline_score,
                candidate_score: r.candidate_score,
                delta: r.delta,
            })
            .collect();

        // 3. Aggregate per axis. `BTreeMap` keeps the per_axis
        //    order stable (matches the SQL `ORDER BY axis ASC`).
        let mut by_axis: std::collections::BTreeMap<String, Vec<&DeltaRow>> =
            std::collections::BTreeMap::new();
        for row in &rows {
            by_axis.entry(row.axis.clone()).or_default().push(row);
        }
        let mut per_axis = Vec::with_capacity(by_axis.len());
        for (axis, group) in by_axis {
            let n = group.len() as f64;
            let baseline_sum: f64 = group.iter().map(|r| r.baseline_score).sum();
            let candidate_sum: f64 = group.iter().map(|r| r.candidate_score).sum();
            let baseline_mean = baseline_sum / n;
            let candidate_mean = candidate_sum / n;
            let mean_delta = candidate_mean - baseline_mean;

            // Tie-counts-half so the win-rate doesn't push to
            // either pole on identical-score runs.
            let wins: f64 = group
                .iter()
                .map(|r| {
                    if r.candidate_score > r.baseline_score {
                        1.0
                    } else if r.candidate_score == r.baseline_score {
                        0.5
                    } else {
                        0.0
                    }
                })
                .sum();
            let win_rate_pct = (wins / n) * 100.0;

            // Cohen's d — pooled-std effect size. `n - 1` Bessel
            // correction on each side; `None` when either side
            // is a single sample (variance undefined under
            // n = 1) or both runs produced identical
            // distributions (zero pooled std).
            let cohen_d = if (n as usize) >= 2 {
                let baseline_var = group
                    .iter()
                    .map(|r| (r.baseline_score - baseline_mean).powi(2))
                    .sum::<f64>()
                    / (n - 1.0);
                let candidate_var = group
                    .iter()
                    .map(|r| (r.candidate_score - candidate_mean).powi(2))
                    .sum::<f64>()
                    / (n - 1.0);
                let pooled = ((baseline_var + candidate_var) / 2.0).sqrt();
                if pooled > 0.0 {
                    Some(mean_delta / pooled)
                } else {
                    None
                }
            } else {
                None
            };

            per_axis.push(RunAxisSummary {
                axis,
                paired_case_count: n as u64,
                baseline_mean,
                candidate_mean,
                mean_delta,
                win_rate_pct,
                cohen_d,
            });
        }

        Ok(RunComparisonReport {
            baseline_run_id,
            candidate_run_id,
            dataset_id,
            per_case,
            per_axis,
        })
    }

    // --- Cases ---------------------------------------------------------

    #[tracing::instrument(level = "debug", skip_all, fields(run_id = %case.run_id, case_key = %case.case_key))]
    async fn upsert_evaluation_case(&self, case: &EvaluationCase) -> OxResult<EvaluationCase> {
        let workspace_id = super::bound_workspace_id_for_dml()?;
        let row: EvaluationCaseRow = sqlx::query_as(
            "INSERT INTO evaluation_cases
                (id, run_id, workspace_id, case_key, input, expected, actual,
                 error, latency_ms, metadata, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
             ON CONFLICT (run_id, case_key) DO UPDATE SET
                input = EXCLUDED.input,
                expected = EXCLUDED.expected,
                actual = EXCLUDED.actual,
                error = EXCLUDED.error,
                latency_ms = EXCLUDED.latency_ms,
                metadata = EXCLUDED.metadata
             RETURNING id, run_id, workspace_id, case_key, input, expected,
                       actual, error, latency_ms, metadata, created_at",
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
        .bind(&case.metadata)
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
                    error, latency_ms, metadata, created_at
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
                    error, latency_ms, metadata, created_at
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

    #[tracing::instrument(level = "debug", skip_all, fields(metric_kind = %metric_kind, limit = limit))]
    async fn list_unjudged_cases(
        &self,
        metric_kind: &str,
        limit: u32,
    ) -> OxResult<Vec<EvaluationCase>> {
        // Cross-workspace surface — runs under SYSTEM_BYPASS so the
        // worker fans out across every tenant in one tick. The
        // anti-join via NOT EXISTS picks cases with `actual` set
        // and zero metrics tagged `metadata.kind = $metric_kind`.
        // `retrieve_anchors` cases skip via the input-shape probe —
        // they're scored deterministically at execute time and
        // don't benefit from an LLM judge round-trip. Hard cap of
        // 500 prevents a backlog spike from OOM-ing the worker.
        let capped = (limit.max(1)).min(500) as i64;
        let rows: Vec<EvaluationCaseRow> = sqlx::query_as(
            "SELECT c.id, c.run_id, c.workspace_id, c.case_key, c.input,
                    c.expected, c.actual, c.error, c.latency_ms, c.metadata,
                    c.created_at
             FROM evaluation_cases c
             WHERE c.actual IS NOT NULL
               AND c.error IS NULL
               AND COALESCE(c.input ->> 'kind', '') <> 'retrieve_anchors'
               AND NOT EXISTS (
                   SELECT 1 FROM evaluation_metrics m
                    WHERE m.case_id = c.id
                      AND m.metadata ->> 'kind' = $1
               )
             ORDER BY c.created_at ASC
             LIMIT $2",
        )
        .bind(metric_kind)
        .bind(capped)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(rows.into_iter().map(EvaluationCase::from).collect())
    }

    // --- Metrics -------------------------------------------------------

    #[tracing::instrument(level = "debug", skip_all, fields(case_id = %metric.case_id, metric.name = %metric.name))]
    async fn upsert_evaluation_metric(
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
/// `upsert_evaluation_metric` call rather than silently
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
        self.record_metric(ctx, format!("latency_ms.{operation}"), latency_ms as f64, "latency_ms", operation)
            .await
    }

    #[tracing::instrument(level = "debug", skip_all, fields(
        case_id = %ctx.case_id,
        operation = %operation,
        prompt = prompt_tokens,
        completion = completion_tokens,
    ))]
    async fn record_tokens(
        &self,
        ctx: &EvaluationContext,
        operation: &str,
        prompt_tokens: u32,
        completion_tokens: u32,
    ) -> OxResult<()> {
        // Two rows — prompt + completion. Splitting on the
        // metric name lets the FE roll up per-axis (ratio of
        // prompt:completion is a meaningful fingerprint of how
        // chatty the system prompt is) without a second pass.
        self.record_metric(
            ctx,
            format!("tokens.prompt.{operation}"),
            prompt_tokens as f64,
            "tokens",
            operation,
        )
        .await?;
        self.record_metric(
            ctx,
            format!("tokens.completion.{operation}"),
            completion_tokens as f64,
            "tokens",
            operation,
        )
        .await
    }

    #[tracing::instrument(level = "debug", skip_all, fields(
        case_id = %ctx.case_id,
        operation = %operation,
        cost_micro_usd = cost_micro_usd,
    ))]
    async fn record_cost_usd(
        &self,
        ctx: &EvaluationContext,
        operation: &str,
        cost_micro_usd: u64,
    ) -> OxResult<()> {
        // Stored in micro-USD on the metric `score: f64` — keeps
        // the wire shape uniform with latency / tokens (numeric
        // axis). Sub-cent precision is meaningful for high-volume
        // operations (an embedding call at 0.0001 USD per 1K
        // tokens flattens to "0.00" if stored in cents).
        self.record_metric(
            ctx,
            format!("cost_usd.{operation}"),
            cost_micro_usd as f64,
            "cost_usd",
            operation,
        )
        .await
    }
}

impl PostgresStore {
    /// Shared write path for every numeric `EvaluationCapture`
    /// metric. Stamps the workspace from the bound task-local,
    /// builds a uniform `metadata` envelope (`kind`, `operation`,
    /// run + case correlation), and lands the row through
    /// `super::EvaluationStore::upsert_evaluation_metric` so re-runs
    /// replace in place on the natural key `(case_id, name)`.
    async fn record_metric(
        &self,
        ctx: &EvaluationContext,
        name: String,
        score: f64,
        kind: &'static str,
        operation: &str,
    ) -> OxResult<()> {
        let workspace_id = super::bound_workspace_id_for_dml()?;
        let metric = EvaluationMetric {
            id: Uuid::now_v7(),
            case_id: ctx.case_id,
            workspace_id,
            name,
            score,
            reasoning: None,
            metadata: serde_json::json!({
                "kind": kind,
                "operation": operation,
                "run_id": ctx.run_id,
                "case_key": ctx.case_key,
            }),
            created_at: chrono::Utc::now(),
        };
        self.upsert_evaluation_metric(&metric).await.map(|_| ())
    }
}
