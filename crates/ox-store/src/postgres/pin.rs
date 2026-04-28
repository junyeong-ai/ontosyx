//! [`PinStore`] — pinboard — saved query results gated on execution ownership.

use super::*;

#[async_trait]
impl PinStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn create_pin(&self, user_id: &str, item: &PinboardItem) -> OxResult<()> {
        super::require_workspace_context()?;
        // Verify ownership: query_execution must belong to the principal
        let result = sqlx::query(
            "INSERT INTO pinboard_items (id, query_execution_id, user_id, widget_spec, title, pinned_at)
             SELECT $1, $2, $6, $3, $4, $5
             FROM query_executions
             WHERE id = $2 AND user_id = $6",
        )
        .bind(item.id)
        .bind(item.query_execution_id)
        .bind(&item.widget_spec)
        .bind(&item.title)
        .bind(item.pinned_at)
        .bind(user_id)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        if result.rows_affected() == 0 {
            return Err(OxError::NotFound {
                entity: "QueryExecution".to_string(),
            });
        }
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_pins(
        &self,
        user_id: &str,
        pagination: &CursorParams,
    ) -> OxResult<CursorPage<PinboardItem>> {
        let limit = pagination.effective_limit();
        let fetch_limit = limit + 1;

        let rows = match pagination.cursor_parts() {
            Some((cursor_ts, cursor_id)) => sqlx::query_as::<_, PinboardItem>(
                "SELECT *
                 FROM pinboard_items
                 WHERE user_id = $1
                   AND (pinned_at, id) < ($2, $3)
                 ORDER BY pinned_at DESC, id DESC
                 LIMIT $4",
            )
            .bind(user_id)
            .bind(cursor_ts)
            .bind(cursor_id)
            .bind(fetch_limit)
            .fetch_all(&self.pool)
            .await
            .map_err(to_ox_error)?,
            None => sqlx::query_as::<_, PinboardItem>(
                "SELECT *
                 FROM pinboard_items
                 WHERE user_id = $1
                 ORDER BY pinned_at DESC, id DESC
                 LIMIT $2",
            )
            .bind(user_id)
            .bind(fetch_limit)
            .fetch_all(&self.pool)
            .await
            .map_err(to_ox_error)?,
        };

        Ok(build_cursor_page(rows, limit, |p| (p.pinned_at, p.id)))
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn delete_pin(&self, user_id: &str, id: Uuid) -> OxResult<bool> {
        super::require_workspace_context()?;
        let result = sqlx::query(
            "DELETE FROM pinboard_items
             WHERE id = $1 AND user_id = $2",
        )
        .bind(id)
        .bind(user_id)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(result.rows_affected() > 0)
    }
}
