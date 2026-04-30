//! [`LoadCheckpointStore`] — watermark tracking for incremental load jobs.

use super::*;

/// Crate-private row mirror for `load_checkpoints`. Lives here only
/// so [`Self::into_domain`] lifts persisted rows back to the
/// canonical [`LoadCheckpoint`] (which carries `id` / `workspace_id`
/// as `Option<Uuid>` reflecting authored-vs-persisted state) in one
/// place.
#[derive(sqlx::FromRow)]
struct LoadCheckpointRow {
    id: Uuid,
    workspace_id: Uuid,
    project_id: Uuid,
    source_table: String,
    graph_label: String,
    watermark_column: String,
    watermark_value: String,
    record_count: i64,
    loaded_at: chrono::DateTime<chrono::Utc>,
}

impl LoadCheckpointRow {
    fn into_domain(self) -> LoadCheckpoint {
        LoadCheckpoint {
            id: Some(self.id),
            workspace_id: Some(self.workspace_id),
            project_id: self.project_id,
            source_table: self.source_table,
            graph_label: self.graph_label,
            watermark_column: self.watermark_column,
            watermark_value: self.watermark_value,
            record_count: self.record_count,
            loaded_at: self.loaded_at,
        }
    }
}

#[async_trait]
impl LoadCheckpointStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_load_checkpoint(
        &self,
        project_id: Uuid,
        source_table: &str,
        graph_label: &str,
    ) -> OxResult<Option<LoadCheckpoint>> {
        let row: Option<LoadCheckpointRow> = sqlx::query_as(
            "SELECT id, workspace_id, project_id, source_table, graph_label,
                    watermark_column, watermark_value, record_count, loaded_at
             FROM load_checkpoints
             WHERE project_id = $1 AND source_table = $2 AND graph_label = $3",
        )
        .bind(project_id)
        .bind(source_table)
        .bind(graph_label)
        .fetch_optional(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(row.map(LoadCheckpointRow::into_domain))
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn upsert_load_checkpoint(&self, c: &LoadCheckpoint) -> OxResult<()> {
        // Bind workspace_id from the active task-local rather than
        // the caller-supplied field — RLS enforces row.workspace_id =
        // current_setting('app.workspace_id'), and the table's `id`
        // column carries `DEFAULT gen_random_uuid()` so the surrogate
        // key falls out of the schema, not the caller. ADR-0039.
        let workspace_id = super::bound_workspace_id_for_dml()?;
        sqlx::query(
            "INSERT INTO load_checkpoints
             (workspace_id, project_id, source_table, graph_label,
              watermark_column, watermark_value, record_count, loaded_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
             ON CONFLICT (workspace_id, project_id, source_table, graph_label)
             DO UPDATE SET
                watermark_column = EXCLUDED.watermark_column,
                watermark_value = EXCLUDED.watermark_value,
                record_count = load_checkpoints.record_count + EXCLUDED.record_count,
                loaded_at = EXCLUDED.loaded_at",
        )
        .bind(workspace_id)
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
    async fn list_load_checkpoints(&self, project_id: Uuid) -> OxResult<Vec<LoadCheckpoint>> {
        let rows: Vec<LoadCheckpointRow> = sqlx::query_as(
            "SELECT id, workspace_id, project_id, source_table, graph_label,
                    watermark_column, watermark_value, record_count, loaded_at
             FROM load_checkpoints
             WHERE project_id = $1
             ORDER BY loaded_at DESC",
        )
        .bind(project_id)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(rows.into_iter().map(LoadCheckpointRow::into_domain).collect())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn delete_load_checkpoint(&self, id: Uuid) -> OxResult<bool> {
        super::require_workspace_context()?;
        let result = sqlx::query("DELETE FROM load_checkpoints WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(to_ox_error)?;
        Ok(result.rows_affected() > 0)
    }
}
