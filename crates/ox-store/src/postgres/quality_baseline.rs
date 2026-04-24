//! [`QualityBaselineStore`] — per-workspace quality baseline thresholds (adaptive fallback).

use super::*;

#[async_trait]
impl crate::store::QualityBaselineStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn upsert_quality_baseline(
        &self,
        baseline: &crate::quality_signal::WorkspaceQualityBaseline,
    ) -> OxResult<()> {
        sqlx::query(
            "INSERT INTO workspace_quality_baseline
                 (workspace_id, window_label, sample_size, thresholds, computed_at)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (workspace_id) DO UPDATE SET
                 window_label = EXCLUDED.window_label,
                 sample_size = EXCLUDED.sample_size,
                 thresholds = EXCLUDED.thresholds,
                 computed_at = EXCLUDED.computed_at",
        )
        .bind(baseline.workspace_id)
        .bind(&baseline.window_label)
        .bind(baseline.sample_size)
        .bind(&baseline.thresholds)
        .bind(baseline.computed_at)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_quality_baseline(
        &self,
    ) -> OxResult<Option<crate::quality_signal::WorkspaceQualityBaseline>> {
        // RLS scopes the query to the current workspace; the cron
        // writes under `WORKSPACE_ID.scope(ws_id, …)` per workspace,
        // and the banner reads under the request's workspace scope.
        sqlx::query_as::<_, crate::quality_signal::WorkspaceQualityBaseline>(
            "SELECT workspace_id, window_label, sample_size, thresholds, computed_at
             FROM workspace_quality_baseline
             LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)
    }
}
