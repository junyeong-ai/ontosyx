//! [`LoadCheckpointStore`] — watermark tracking for incremental load jobs.

use super::*;

#[async_trait]
impl LoadCheckpointStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_checkpoint(
        &self,
        project_id: Uuid,
        source_table: &str,
        graph_label: &str,
    ) -> OxResult<Option<LoadCheckpoint>> {
        sqlx::query_as(
            "SELECT * FROM load_checkpoints
             WHERE project_id = $1 AND source_table = $2 AND graph_label = $3",
        )
        .bind(project_id)
        .bind(source_table)
        .bind(graph_label)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn upsert_checkpoint(&self, c: &LoadCheckpoint) -> OxResult<()> {
        sqlx::query(
            "INSERT INTO load_checkpoints
             (id, workspace_id, project_id, source_table, graph_label,
              watermark_column, watermark_value, record_count, loaded_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
             ON CONFLICT (workspace_id, project_id, source_table, graph_label)
             DO UPDATE SET
                watermark_column = EXCLUDED.watermark_column,
                watermark_value = EXCLUDED.watermark_value,
                record_count = load_checkpoints.record_count + EXCLUDED.record_count,
                loaded_at = EXCLUDED.loaded_at",
        )
        .bind(c.id)
        .bind(c.workspace_id)
        .bind(c.project_id)
        .bind(&c.source_table)
        .bind(&c.graph_label)
        .bind(&c.watermark_column)
        .bind(&c.watermark_value)
        .bind(c.record_count)
        .bind(c.loaded_at)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_checkpoints(&self, project_id: Uuid) -> OxResult<Vec<LoadCheckpoint>> {
        sqlx::query_as(
            "SELECT * FROM load_checkpoints
             WHERE project_id = $1
             ORDER BY loaded_at DESC",
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn delete_checkpoint(&self, id: Uuid) -> OxResult<bool> {
        let result = sqlx::query("DELETE FROM load_checkpoints WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        Ok(result.rows_affected() > 0)
    }
}
