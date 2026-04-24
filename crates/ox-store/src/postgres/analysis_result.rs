//! [`AnalysisResultStore`] — cached analysis recipe outputs keyed by input hash.

use super::*;

#[async_trait]
impl AnalysisResultStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn create_analysis_result(&self, r: &AnalysisResult) -> OxResult<()> {
        sqlx::query(
            "INSERT INTO analysis_results (id, recipe_id, ontology_lineage_id, input_hash, output, duration_ms, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(r.id)
        .bind(r.recipe_id)
        .bind(&r.ontology_lineage_id)
        .bind(&r.input_hash)
        .bind(&r.output)
        .bind(r.duration_ms)
        .bind(r.created_at)
        .execute(&self.pool)
        .await
        .map_err(|e| OxError::Runtime {
            message: format!("Database error: {e}"),
        })?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_cached_result(
        &self,
        input_hash: &str,
        recipe_id: Option<Uuid>,
    ) -> OxResult<Option<AnalysisResult>> {
        let result = if let Some(rid) = recipe_id {
            sqlx::query_as(
                "SELECT * FROM analysis_results WHERE input_hash = $1 AND recipe_id = $2
                 ORDER BY created_at DESC LIMIT 1",
            )
            .bind(input_hash)
            .bind(rid)
            .fetch_optional(&self.pool)
            .await
        } else {
            sqlx::query_as(
                "SELECT * FROM analysis_results WHERE input_hash = $1 AND recipe_id IS NULL
                 ORDER BY created_at DESC LIMIT 1",
            )
            .bind(input_hash)
            .fetch_optional(&self.pool)
            .await
        }
        .map_err(|e| OxError::Runtime {
            message: format!("Database error: {e}"),
        })?;
        Ok(result)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_analysis_results(
        &self,
        recipe_id: Uuid,
        limit: i64,
    ) -> OxResult<Vec<AnalysisResult>> {
        sqlx::query_as(
            "SELECT * FROM analysis_results WHERE recipe_id = $1
             ORDER BY created_at DESC LIMIT $2",
        )
        .bind(recipe_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| OxError::Runtime {
            message: format!("Database error: {e}"),
        })
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn cleanup_old_results(&self, max_age_days: i64) -> OxResult<Vec<(Uuid, u64)>> {
        let rows: Vec<(Uuid, i64)> = sqlx::query_as(
            "WITH affected AS (
                 DELETE FROM analysis_results
                 WHERE created_at < NOW() - make_interval(days => $1)
                 RETURNING workspace_id
             )
             SELECT workspace_id, COUNT(*)::bigint
             FROM affected
             GROUP BY workspace_id",
        )
        .bind(max_age_days as i32)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(rows.into_iter().map(|(ws, n)| (ws, n as u64)).collect())
    }
}
