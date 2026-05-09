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

use ox_ontology::{EvaluationFingerprint, ModelCall, ModelId, ModelPrices};

use crate::evaluation::{
    EvaluationActual, EvaluationCapture, EvaluationCaptureAxis, EvaluationCase,
    EvaluationCaseInput, EvaluationCaseMetadata, EvaluationContext, EvaluationDataset,
    EvaluationDatasetItem, EvaluationExpected, EvaluationMetric, EvaluationMetricMetadata,
    EvaluationRun, EvaluationRunStatus, parse_run_status,
};
use crate::store::{CursorPage, CursorParams, EvaluationStore};

use super::*;

/// Crate-private row mirror for `evaluation_runs`. Keeps the
/// `status` decode + the fingerprint hydration in one place.
#[derive(sqlx::FromRow)]
struct EvaluationRunRow {
    id: Uuid,
    workspace_id: Uuid,
    fingerprint_components: sqlx::types::Json<EvaluationFingerprint>,
    fingerprint_digest: String,
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
            fingerprint: self.fingerprint_components.0,
            fingerprint_digest: self.fingerprint_digest,
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

/// Row mirror for `evaluation_datasets` LEFT JOIN
/// `evaluation_dataset_items`. The COUNT(*) lands as `bigint`
/// in Postgres → `i64` in Rust; the conversion to the
/// domain-level `u64` clamps negatives (impossible from
/// COUNT but defensive).
#[derive(sqlx::FromRow)]
struct EvaluationDatasetWithCountRow {
    id: Uuid,
    workspace_id: Uuid,
    name: String,
    description: String,
    created_at: chrono::DateTime<chrono::Utc>,
    item_count: i64,
}

/// Row mirror for `evaluation_dataset_items`.
#[derive(sqlx::FromRow)]
struct EvaluationDatasetItemRow {
    id: Uuid,
    dataset_id: Uuid,
    workspace_id: Uuid,
    item_key: String,
    input: sqlx::types::Json<EvaluationCaseInput>,
    expected: Option<sqlx::types::Json<EvaluationExpected>>,
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
            input: r.input.0,
            expected: r.expected.map(|expected| expected.0),
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
    input: sqlx::types::Json<EvaluationCaseInput>,
    expected: Option<sqlx::types::Json<EvaluationExpected>>,
    actual: Option<sqlx::types::Json<EvaluationActual>>,
    error: Option<String>,
    latency_ms: Option<i64>,
    metadata: sqlx::types::Json<EvaluationCaseMetadata>,
    created_at: chrono::DateTime<chrono::Utc>,
}

impl From<EvaluationCaseRow> for EvaluationCase {
    fn from(r: EvaluationCaseRow) -> Self {
        Self {
            id: r.id,
            run_id: r.run_id,
            workspace_id: r.workspace_id,
            case_key: r.case_key,
            input: r.input.0,
            expected: r.expected.map(|expected| expected.0),
            actual: r.actual.map(|actual| actual.0),
            error: r.error,
            latency_ms: r.latency_ms,
            metadata: r.metadata.0,
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
    metadata: sqlx::types::Json<EvaluationMetricMetadata>,
    provenance_id: Option<Uuid>,
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
            metadata: r.metadata.0,
            provenance_id: r.provenance_id,
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
    async fn get_evaluation_dataset(&self, id: Uuid) -> OxResult<Option<EvaluationDataset>> {
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
    ) -> OxResult<CursorPage<crate::evaluation::EvaluationDatasetSummary>> {
        super::require_workspace_context()?;
        let limit = pagination.effective_limit();
        let fetch_limit = limit + 1;

        // Single round-trip: dataset header + LEFT JOIN
        // COUNT(*) per dataset_id. The COUNT is grouped by
        // every header column the SELECT projects, so the
        // GROUP BY mirrors that — Postgres rejects an
        // aggregate without grouping the non-aggregated
        // columns. LEFT JOIN keeps datasets with zero items in
        // the page (the COUNT collapses to 0 when no item
        // rows match).
        let mut qb: sqlx::QueryBuilder<sqlx::Postgres> = sqlx::QueryBuilder::new(
            "SELECT d.id, d.workspace_id, d.name, d.description, d.created_at,
                    COUNT(i.id)::bigint AS item_count
             FROM evaluation_datasets d
             LEFT JOIN evaluation_dataset_items i ON i.dataset_id = d.id
             WHERE TRUE",
        );
        if let Some((cursor_ts, cursor_id)) = pagination.cursor_parts() {
            qb.push(" AND (d.created_at, d.id) < (");
            qb.push_bind(cursor_ts);
            qb.push(", ");
            qb.push_bind(cursor_id);
            qb.push(")");
        }
        qb.push(" GROUP BY d.id, d.workspace_id, d.name, d.description, d.created_at");
        qb.push(" ORDER BY d.created_at DESC, d.id DESC LIMIT ");
        qb.push_bind(fetch_limit);

        let rows: Vec<EvaluationDatasetWithCountRow> = qb
            .build_query_as()
            .fetch_all(&self.pool)
            .await
            .map_err(to_ox_error)?;
        let items: Vec<crate::evaluation::EvaluationDatasetSummary> = rows
            .into_iter()
            .map(|r| crate::evaluation::EvaluationDatasetSummary {
                dataset: EvaluationDataset {
                    id: r.id,
                    workspace_id: r.workspace_id,
                    name: r.name,
                    description: r.description,
                    created_at: r.created_at,
                },
                item_count: r.item_count.max(0) as u64,
            })
            .collect();
        Ok(super::build_cursor_page(items, limit, |s| {
            (s.dataset.created_at, s.dataset.id)
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
        .bind(sqlx::types::Json(&item.input))
        .bind(item.expected.as_ref().map(sqlx::types::Json))
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
            .bind(sqlx::types::Json(&item.input))
            .bind(item.expected.as_ref().map(sqlx::types::Json))
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

    #[tracing::instrument(level = "debug", skip_all, fields(
        dataset_id = %fingerprint.dataset_id,
        ontology_version_id = %fingerprint.ontology_version_id,
        run.name = %run_name,
    ))]
    async fn create_run_from_dataset(
        &self,
        run_name: &str,
        run_description: &str,
        fingerprint: EvaluationFingerprint,
        run_metadata: serde_json::Value,
    ) -> OxResult<(EvaluationRun, u64)> {
        let workspace_id = super::bound_workspace_id_for_dml()?;
        let dataset_id = fingerprint.dataset_id;
        let ontology_version_id = fingerprint.ontology_version_id;
        let model_id_str = fingerprint.model_id.as_str().to_string();
        let fingerprint_digest = fingerprint.digest()?;
        let fingerprint_json = serde_json::to_value(&fingerprint).map_err(|e| OxError::Runtime {
            message: format!("EvaluationFingerprint serialise failed: {e}"),
        })?;
        let mut tx = self.pool.begin().await.map_err(to_ox_error)?;

        // Verify the dataset exists in the active workspace
        // before consuming it. RLS already filters cross-tenant
        // ids, but a typed `NotFound` reads cleaner than a
        // confusing "0 items" run.
        let exists: Option<(Uuid,)> =
            sqlx::query_as("SELECT id FROM evaluation_datasets WHERE id = $1")
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
                (id, workspace_id, ontology_version_id, dataset_id, model_id,
                 fingerprint_digest, fingerprint_components,
                 name, description, status, started_at, completed_at, metadata)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 'running', $10, NULL, $11)
             RETURNING id, workspace_id, fingerprint_components, fingerprint_digest,
                       name, description, status, started_at, completed_at, metadata",
        )
        .bind(run_id)
        .bind(workspace_id)
        .bind(ontology_version_id)
        .bind(dataset_id)
        .bind(&model_id_str)
        .bind(&fingerprint_digest)
        .bind(&fingerprint_json)
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
        // Recompute the digest at write time so a malformed caller
        // cannot persist a digest that does not match the
        // components — the stored pair is always coherent.
        let digest = run.fingerprint.digest()?;
        let components = serde_json::to_value(&run.fingerprint).map_err(|e| OxError::Runtime {
            message: format!("EvaluationFingerprint serialise failed: {e}"),
        })?;
        let row: EvaluationRunRow = sqlx::query_as(
            "INSERT INTO evaluation_runs
                (id, workspace_id, ontology_version_id, dataset_id, model_id,
                 fingerprint_digest, fingerprint_components,
                 name, description, status, started_at, completed_at, metadata)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
             RETURNING id, workspace_id, fingerprint_components, fingerprint_digest,
                       name, description, status, started_at, completed_at, metadata",
        )
        .bind(run.id)
        .bind(workspace_id)
        .bind(run.fingerprint.ontology_version_id)
        .bind(run.fingerprint.dataset_id)
        .bind(run.fingerprint.model_id.as_str())
        .bind(&digest)
        .bind(&components)
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
            "SELECT id, workspace_id, fingerprint_components, fingerprint_digest,
                    name, description, status, started_at, completed_at, metadata
             FROM evaluation_runs WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)?;
        row.map(EvaluationRunRow::into_domain).transpose()
    }

    #[tracing::instrument(level = "debug", skip_all, fields(name = %name))]
    async fn find_evaluation_run_by_name(&self, name: &str) -> OxResult<Option<EvaluationRun>> {
        super::require_workspace_context()?;
        let row: Option<EvaluationRunRow> = sqlx::query_as(
            "SELECT id, workspace_id, fingerprint_components, fingerprint_digest,
                    name, description, status, started_at, completed_at, metadata
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
            "SELECT id, workspace_id, fingerprint_components, fingerprint_digest,
                    name, description, status, started_at, completed_at, metadata
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
             RETURNING id, workspace_id, fingerprint_components, fingerprint_digest,
                       name, description, status, started_at, completed_at, metadata",
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

    #[tracing::instrument(level = "debug", skip_all, fields(run_id = %run_id))]
    async fn evaluation_run_summary(
        &self,
        run_id: Uuid,
    ) -> OxResult<crate::evaluation::RunSummary> {
        use crate::evaluation::{
            AxisAggregate, RetrievalComparisonAggregate, RetrievalSurface, RunSummary,
        };
        super::require_workspace_context()?;

        // Single round-trip — three SELECTs against
        // `evaluation_cases` + `evaluation_metrics`, joined
        // workspace-side. RLS scopes both tables; the inner
        // probes never see cross-tenant rows.
        //
        // `judged_cases` counts cases (not metrics) that have
        // any RAGAS-tagged metric — the COUNT DISTINCT prevents
        // a case with 4 axes from inflating the count by 4.
        // `failed_cases` is a separate count over the cases row.
        let (total_cases, judged_cases, failed_cases): (i64, i64, i64) = sqlx::query_as(
            "SELECT
                    (SELECT COUNT(*)
                       FROM evaluation_cases
                      WHERE run_id = $1) AS total,
                    (SELECT COUNT(DISTINCT m.case_id)
                       FROM evaluation_metrics m
                       JOIN evaluation_cases c ON c.id = m.case_id
                      WHERE c.run_id = $1
                        AND m.metadata ->> 'kind' = 'judge') AS judged,
                    (SELECT COUNT(*)
                       FROM evaluation_cases
                      WHERE run_id = $1
                        AND error IS NOT NULL) AS failed",
        )
        .bind(run_id)
        .fetch_one(&self.pool)
        .await
        .map_err(to_ox_error)?;

        // Per-axis aggregate across every metric attached to
        // every case in the run. `axis ASC` sort keeps the FE
        // column ordering stable across re-fetches.
        let axis_rows: Vec<(String, f64, i64)> = sqlx::query_as(
            "SELECT m.name AS axis,
                    AVG(m.score)::float8 AS mean,
                    COUNT(*) AS cnt
               FROM evaluation_metrics m
               JOIN evaluation_cases c ON c.id = m.case_id
              WHERE c.run_id = $1
              GROUP BY m.name
              ORDER BY m.name ASC",
        )
        .bind(run_id)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)?;

        // Per-(surface, axis) hybrid-vs-trigram aggregate. The
        // `<surface>.<leg>.<axis>` naming convention is parsed
        // server-side via SPLIT_PART; the FE only sees the
        // typed shape. Pairing is intra-case via
        // `MAX(CASE WHEN leg = … THEN score)` so every paired
        // row carries both legs. The `HAVING` clause drops
        // singletons (cases that produced only one leg) so the
        // denominator stays honest.
        let comparison_rows: Vec<(String, String, i64, f64, f64, f64, f64)> = sqlx::query_as(
            "WITH parsed AS (
                SELECT
                  c.id AS case_id,
                  SPLIT_PART(m.name, '.', 1) AS surface,
                  SPLIT_PART(m.name, '.', 2) AS leg,
                  SPLIT_PART(m.name, '.', 3) AS axis,
                  m.score
                FROM evaluation_metrics m
                JOIN evaluation_cases c ON c.id = m.case_id
                WHERE c.run_id = $1
                  AND m.name LIKE '%.%.%'
            ),
            paired AS (
                SELECT
                  case_id,
                  surface,
                  axis,
                  MAX(CASE WHEN leg = 'hybrid'  THEN score END) AS hybrid_score,
                  MAX(CASE WHEN leg = 'trigram' THEN score END) AS trigram_score
                FROM parsed
                WHERE surface IN ('verified_query', 'community_summary', 'knowledge_entry')
                  AND leg IN ('hybrid', 'trigram')
                  AND axis IN ('precision_at_k', 'recall_at_k', 'mrr', 'ndcg_at_k')
                GROUP BY case_id, surface, axis
                HAVING MAX(CASE WHEN leg = 'hybrid'  THEN score END) IS NOT NULL
                   AND MAX(CASE WHEN leg = 'trigram' THEN score END) IS NOT NULL
            )
            SELECT
              surface,
              axis,
              COUNT(*)::int8 AS paired_case_count,
              AVG(hybrid_score)::float8 AS hybrid_mean,
              AVG(trigram_score)::float8 AS trigram_mean,
              AVG(hybrid_score - trigram_score)::float8 AS mean_lift,
              (100.0 * AVG(
                  CASE
                    WHEN hybrid_score > trigram_score THEN 1.0
                    WHEN hybrid_score = trigram_score THEN 0.5
                    ELSE 0.0
                  END
              ))::float8 AS win_rate_pct
            FROM paired
            GROUP BY surface, axis
            ORDER BY surface ASC, axis ASC",
        )
        .bind(run_id)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)?;

        let retrieval_comparisons: Vec<RetrievalComparisonAggregate> = comparison_rows
            .into_iter()
            .filter_map(
                |(surface, axis, paired, hybrid_mean, trigram_mean, mean_lift, win_rate)| {
                    let surface = match surface.as_str() {
                        "verified_query" => RetrievalSurface::VerifiedQuery,
                        "community_summary" => RetrievalSurface::CommunitySummary,
                        "knowledge_entry" => RetrievalSurface::KnowledgeEntry,
                        // Forward-compat: a metric with a known leg+axis
                        // but unknown surface (e.g. a future surface
                        // landed before the build was bumped) drops
                        // out of the typed aggregate rather than
                        // crashing the run-summary call. Operators
                        // still see the raw rows in `axis_means`.
                        _ => return None,
                    };
                    Some(RetrievalComparisonAggregate {
                        surface,
                        axis,
                        paired_case_count: paired.max(0) as u64,
                        hybrid_mean,
                        trigram_mean,
                        mean_lift,
                        win_rate_pct: win_rate,
                    })
                },
            )
            .collect();

        Ok(RunSummary {
            run_id,
            total_cases: total_cases.max(0) as u64,
            judged_cases: judged_cases.max(0) as u64,
            failed_cases: failed_cases.max(0) as u64,
            axis_means: axis_rows
                .into_iter()
                .map(|(axis, mean, cnt)| AxisAggregate {
                    axis,
                    mean,
                    count: cnt.max(0) as u64,
                })
                .collect(),
            retrieval_comparisons,
        })
    }

    #[tracing::instrument(level = "debug", skip_all, fields(
        baseline = %baseline_run_id,
        candidate = %candidate_run_id,
    ))]
    async fn compare_evaluation_runs(
        &self,
        baseline_run_id: Uuid,
        candidate_run_id: Uuid,
        regression_policy: crate::evaluation::RegressionPolicy,
    ) -> OxResult<crate::evaluation::RunComparisonReport> {
        use crate::evaluation::{RunAxisSummary, RunComparisonReport, RunMetricDelta};
        super::require_workspace_context()?;

        // 1. Verify both runs exist + share dataset_id. RLS
        //    already filters cross-tenant ids; the dataset
        //    correspondence check is the pair gate. `dataset_id`
        //    is `NOT NULL` on `evaluation_runs` (enforced through
        //    `EvaluationFingerprint`) so the only failure mode is
        //    "row missing" — a present row always carries a
        //    dataset.
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
                return Err(OxError::NotFound {
                    entity: format!(
                        "evaluation_runs pair baseline={baseline_run_id} candidate={candidate_run_id}"
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

        // Per-(surface, axis) hybrid-lift delta between the two
        // runs. Single round-trip: fold each run's
        // `<surface>.<leg>.<axis>` rows into `(run_role,
        // surface, axis) → mean_lift` and pair them in the
        // outer SELECT. A run that has no `retrieval_comparison`
        // cases simply contributes no rows; the LEFT JOIN drops
        // cells with no overlap so the FE never renders a
        // half-populated row.
        let comparison_rows: Vec<(String, String, f64, f64, f64, i64, i64)> = sqlx::query_as(
            "WITH parsed AS (
                SELECT
                  c.run_id,
                  c.id AS case_id,
                  SPLIT_PART(m.name, '.', 1) AS surface,
                  SPLIT_PART(m.name, '.', 2) AS leg,
                  SPLIT_PART(m.name, '.', 3) AS axis,
                  m.score
                FROM evaluation_metrics m
                JOIN evaluation_cases c ON c.id = m.case_id
                WHERE c.run_id IN ($1, $2)
                  AND m.name LIKE '%.%.%'
            ),
            paired AS (
                SELECT
                  run_id,
                  surface,
                  axis,
                  case_id,
                  MAX(CASE WHEN leg = 'hybrid'  THEN score END) AS hybrid_score,
                  MAX(CASE WHEN leg = 'trigram' THEN score END) AS trigram_score
                FROM parsed
                WHERE surface IN ('verified_query', 'community_summary', 'knowledge_entry')
                  AND leg IN ('hybrid', 'trigram')
                  AND axis IN ('precision_at_k', 'recall_at_k', 'mrr', 'ndcg_at_k')
                GROUP BY run_id, surface, axis, case_id
                HAVING MAX(CASE WHEN leg = 'hybrid'  THEN score END) IS NOT NULL
                   AND MAX(CASE WHEN leg = 'trigram' THEN score END) IS NOT NULL
            ),
            agg AS (
                SELECT
                  run_id,
                  surface,
                  axis,
                  AVG(hybrid_score - trigram_score)::float8 AS mean_lift,
                  COUNT(*)::int8 AS paired_n
                FROM paired
                GROUP BY run_id, surface, axis
            )
            SELECT
              COALESCE(b.surface, c.surface)::text AS surface,
              COALESCE(b.axis,    c.axis)::text    AS axis,
              COALESCE(b.mean_lift, 0.0)::float8   AS baseline_lift,
              COALESCE(c.mean_lift, 0.0)::float8   AS candidate_lift,
              (COALESCE(c.mean_lift, 0.0) - COALESCE(b.mean_lift, 0.0))::float8 AS lift_delta,
              COALESCE(b.paired_n, 0)::int8        AS baseline_paired_n,
              COALESCE(c.paired_n, 0)::int8        AS candidate_paired_n
            FROM (SELECT * FROM agg WHERE run_id = $1) b
            FULL OUTER JOIN (SELECT * FROM agg WHERE run_id = $2) c
              USING (surface, axis)
            ORDER BY surface ASC, axis ASC",
        )
        .bind(baseline_run_id)
        .bind(candidate_run_id)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)?;

        let retrieval_comparison_deltas: Vec<crate::evaluation::RetrievalComparisonDelta> =
            comparison_rows
                .into_iter()
                .filter_map(
                    |(
                        surface,
                        axis,
                        baseline_lift,
                        candidate_lift,
                        lift_delta,
                        baseline_n,
                        candidate_n,
                    )| {
                        let surface = match surface.as_str() {
                            "verified_query" => crate::evaluation::RetrievalSurface::VerifiedQuery,
                            "community_summary" => {
                                crate::evaluation::RetrievalSurface::CommunitySummary
                            }
                            "knowledge_entry" => {
                                crate::evaluation::RetrievalSurface::KnowledgeEntry
                            }
                            _ => return None,
                        };
                        Some(crate::evaluation::RetrievalComparisonDelta {
                            surface,
                            axis,
                            baseline_lift,
                            candidate_lift,
                            lift_delta,
                            baseline_paired_case_count: baseline_n.max(0) as u64,
                            candidate_paired_case_count: candidate_n.max(0) as u64,
                        })
                    },
                )
                .collect();

        // Regression alerts — fold the deltas through a
        // threshold gate. Pure transform; threshold + min-N
        // are platform constants today, workspace-customisable
        // tomorrow without changing the contract.
        let retrieval_lift_regressions: Vec<crate::evaluation::RetrievalLiftRegressionAlert> =
            retrieval_comparison_deltas
                .iter()
                .filter_map(|d| {
                    if d.lift_delta < regression_policy.threshold
                        && d.candidate_paired_case_count
                            >= regression_policy.min_paired_case_count
                    {
                        Some(crate::evaluation::RetrievalLiftRegressionAlert {
                            surface: d.surface,
                            axis: d.axis.clone(),
                            lift_delta: d.lift_delta,
                            baseline_lift: d.baseline_lift,
                            candidate_lift: d.candidate_lift,
                            threshold: regression_policy.threshold,
                            candidate_paired_case_count: d.candidate_paired_case_count,
                        })
                    } else {
                        None
                    }
                })
                .collect();

        Ok(RunComparisonReport {
            baseline_run_id,
            candidate_run_id,
            dataset_id,
            per_case,
            per_axis,
            retrieval_comparison_deltas,
            retrieval_lift_regressions,
        })
    }

    #[tracing::instrument(level = "debug", skip_all, fields(
        run = %run_id, surface = ?surface, axis = ?axis, limit,
    ))]
    async fn list_run_comparison_outliers(
        &self,
        run_id: Uuid,
        surface: Option<crate::evaluation::RetrievalSurface>,
        axis: Option<&str>,
        limit: u32,
    ) -> OxResult<Vec<crate::evaluation::RetrievalComparisonOutlier>> {
        super::require_workspace_context()?;
        let limit_capped = limit.clamp(1, 100) as i64;
        let surface_filter = surface.map(|s| s.as_str());

        // Same paired pivot as the run-level aggregator, with
        // optional filters on surface / axis pushed into the
        // SQL so the planner can use the existing
        // `evaluation_metrics(case_id, name)` index. ORDER BY
        // case_lift ASC surfaces the worst-actor cases first;
        // tie-break on `case_key` keeps the order deterministic
        // across re-fetches.
        let rows: Vec<(Uuid, String, String, String, f64, f64, f64)> = sqlx::query_as(
            "WITH parsed AS (
                SELECT
                  c.id AS case_id,
                  c.case_key,
                  SPLIT_PART(m.name, '.', 1) AS surface,
                  SPLIT_PART(m.name, '.', 2) AS leg,
                  SPLIT_PART(m.name, '.', 3) AS axis,
                  m.score
                FROM evaluation_metrics m
                JOIN evaluation_cases c ON c.id = m.case_id
                WHERE c.run_id = $1
                  AND m.name LIKE '%.%.%'
            ),
            paired AS (
                SELECT
                  case_id,
                  case_key,
                  surface,
                  axis,
                  MAX(CASE WHEN leg = 'hybrid'  THEN score END) AS hybrid_score,
                  MAX(CASE WHEN leg = 'trigram' THEN score END) AS trigram_score
                FROM parsed
                WHERE surface IN ('verified_query', 'community_summary', 'knowledge_entry')
                  AND leg IN ('hybrid', 'trigram')
                  AND axis IN ('precision_at_k', 'recall_at_k', 'mrr', 'ndcg_at_k')
                  AND ($2::text IS NULL OR surface = $2)
                  AND ($3::text IS NULL OR axis = $3)
                GROUP BY case_id, case_key, surface, axis
                HAVING MAX(CASE WHEN leg = 'hybrid'  THEN score END) IS NOT NULL
                   AND MAX(CASE WHEN leg = 'trigram' THEN score END) IS NOT NULL
            )
            SELECT
              case_id,
              case_key,
              surface,
              axis,
              hybrid_score::float8,
              trigram_score::float8,
              (hybrid_score - trigram_score)::float8 AS case_lift
            FROM paired
            ORDER BY (hybrid_score - trigram_score) ASC, case_key ASC
            LIMIT $4",
        )
        .bind(run_id)
        .bind(surface_filter)
        .bind(axis)
        .bind(limit_capped)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)?;

        Ok(rows
            .into_iter()
            .filter_map(
                |(case_id, case_key, surface_str, axis, hybrid_score, trigram_score, case_lift)| {
                    let surface = match surface_str.as_str() {
                        "verified_query" => crate::evaluation::RetrievalSurface::VerifiedQuery,
                        "community_summary" => {
                            crate::evaluation::RetrievalSurface::CommunitySummary
                        }
                        "knowledge_entry" => crate::evaluation::RetrievalSurface::KnowledgeEntry,
                        _ => return None,
                    };
                    Some(crate::evaluation::RetrievalComparisonOutlier {
                        case_id,
                        case_key,
                        surface,
                        axis,
                        hybrid_score,
                        trigram_score,
                        case_lift,
                    })
                },
            )
            .collect())
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
        .bind(sqlx::types::Json(&case.input))
        .bind(case.expected.as_ref().map(sqlx::types::Json))
        .bind(case.actual.as_ref().map(sqlx::types::Json))
        .bind(&case.error)
        .bind(case.latency_ms)
        .bind(sqlx::types::Json(&case.metadata))
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
        let capped = limit.clamp(1, 500) as i64;
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
                 provenance_id, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
             ON CONFLICT (case_id, name) DO UPDATE SET
                score = EXCLUDED.score,
                reasoning = EXCLUDED.reasoning,
                metadata = EXCLUDED.metadata,
                provenance_id = EXCLUDED.provenance_id,
                created_at = EXCLUDED.created_at
             RETURNING id, case_id, workspace_id, name, score, reasoning,
                       metadata, provenance_id, created_at",
        )
        .bind(metric.id)
        .bind(metric.case_id)
        .bind(workspace_id)
        .bind(&metric.name)
        .bind(metric.score)
        .bind(&metric.reasoning)
        .bind(sqlx::types::Json(&metric.metadata))
        .bind(metric.provenance_id)
        .bind(metric.created_at)
        .fetch_one(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(row.into())
    }

    #[tracing::instrument(level = "debug", skip_all, fields(case_id = %case_id))]
    async fn list_evaluation_metrics(&self, case_id: Uuid) -> OxResult<Vec<EvaluationMetric>> {
        super::require_workspace_context()?;
        let rows: Vec<EvaluationMetricRow> = sqlx::query_as(
            "SELECT id, case_id, workspace_id, name, score, reasoning, metadata,
                    provenance_id, created_at
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

/// Storage-backed [`EvaluationCapture`]. One LLM call lands four
/// to five `evaluation_metrics` rows — latency, input / output /
/// cached_input tokens, derived cost — sharing the operation tag
/// so the FE pivots them as a single observation.
///
/// The capture is workspace-scoped via the same task-local
/// guard the rest of the store uses; an evaluation scope without
/// a workspace context fails the underlying
/// `upsert_evaluation_metric` call rather than silently landing
/// rows under a different tenant.
#[async_trait]
impl EvaluationCapture for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all, fields(
        case_id = %ctx.case_id,
        operation = %operation,
        model_id = %call.model_id,
        latency_ms = call.latency_ms,
        input_tokens = call.input_tokens,
        output_tokens = call.output_tokens,
        cached_input_tokens = call.cached_input_tokens,
    ))]
    async fn record_call(
        &self,
        ctx: &EvaluationContext,
        operation: &str,
        call: ModelCall,
    ) -> OxResult<()> {
        self.record_metric(
            ctx,
            format!("latency_ms.{operation}"),
            call.latency_ms as f64,
            EvaluationCaptureAxis::LatencyMs,
            operation,
        )
        .await?;
        self.record_metric(
            ctx,
            format!("tokens.input.{operation}"),
            call.input_tokens as f64,
            EvaluationCaptureAxis::InputTokens,
            operation,
        )
        .await?;
        self.record_metric(
            ctx,
            format!("tokens.output.{operation}"),
            call.output_tokens as f64,
            EvaluationCaptureAxis::OutputTokens,
            operation,
        )
        .await?;
        self.record_metric(
            ctx,
            format!("tokens.cached_input.{operation}"),
            call.cached_input_tokens as f64,
            EvaluationCaptureAxis::CachedInputTokens,
            operation,
        )
        .await?;

        // Cost is derived from the active price row, not stored
        // ahead of time. A miss on the price catalogue skips the
        // cost axis (rather than fabricating a zero) so dashboards
        // distinguish "no price for this model" from "free call".
        if let Some(prices) = self.fetch_active_model_prices(&call.model_id).await? {
            let cost_micro = call.cost_micro_usd(&prices);
            self.record_metric(
                ctx,
                format!("cost_micro_usd.{operation}"),
                cost_micro as f64,
                EvaluationCaptureAxis::CostMicroUsd,
                operation,
            )
            .await?;
        }
        Ok(())
    }
}

impl PostgresStore {
    /// Shared write path for every numeric `EvaluationCapture`
    /// metric row. Stamps the workspace from the bound task-local,
    /// builds a uniform `metadata` envelope (`kind`, `operation`,
    /// run + case correlation), and lands the row through
    /// `super::EvaluationStore::upsert_evaluation_metric` so re-runs
    /// replace in place on the natural key `(case_id, name)`.
    async fn record_metric(
        &self,
        ctx: &EvaluationContext,
        name: String,
        score: f64,
        axis: EvaluationCaptureAxis,
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
            metadata: EvaluationMetricMetadata::Capture {
                axis,
                operation: operation.to_string(),
                run_id: ctx.run_id,
                case_key: ctx.case_key.clone(),
            },
            // Capture-axis observations attach to the case via
            // its `EvaluationCaseMetadata::Call.prompt_render_hash`,
            // not to a metric-level provenance row.
            provenance_id: None,
            created_at: chrono::Utc::now(),
        };
        self.upsert_evaluation_metric(&metric).await.map(|_| ())
    }

    /// Resolve the [`ModelPrices`] row that's authoritative right
    /// now for the supplied `model_id`. Returns `None` when no
    /// row applies — the caller treats the cost axis as absent
    /// rather than synthesising a zero. `model_prices` is platform-
    /// wide reference data (no RLS) so the lookup runs without a
    /// workspace context.
    async fn fetch_active_model_prices(
        &self,
        model_id: &ModelId,
    ) -> OxResult<Option<ModelPrices>> {
        #[derive(sqlx::FromRow)]
        struct ModelPricesRow {
            model_id: String,
            input_price_usd_per_million: f64,
            cached_input_price_usd_per_million: f64,
            output_price_usd_per_million: f64,
            valid_from: chrono::DateTime<chrono::Utc>,
            valid_to: Option<chrono::DateTime<chrono::Utc>>,
        }
        let row: Option<ModelPricesRow> = sqlx::query_as(
            "SELECT model_id, input_price_usd_per_million,
                    cached_input_price_usd_per_million,
                    output_price_usd_per_million,
                    valid_from, valid_to
               FROM model_prices
              WHERE model_id = $1
                AND valid_from <= now()
                AND (valid_to IS NULL OR valid_to > now())
              ORDER BY valid_from DESC
              LIMIT 1",
        )
        .bind(model_id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(row.map(|r| ModelPrices {
            model_id: ModelId::new(r.model_id),
            input_price_usd_per_million: r.input_price_usd_per_million,
            cached_input_price_usd_per_million: r.cached_input_price_usd_per_million,
            output_price_usd_per_million: r.output_price_usd_per_million,
            valid_from: r.valid_from,
            valid_to: r.valid_to,
        }))
    }
}
