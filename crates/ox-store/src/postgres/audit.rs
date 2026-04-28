//! [`AuditStore`] — append-only audit log, cross-workspace readable via affected_workspace_id.

use super::*;

#[async_trait]
impl AuditStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn record_audit(
        &self,
        user_id: Option<Uuid>,
        action: &str,
        resource_type: &str,
        resource_id: Option<&str>,
        details: serde_json::Value,
    ) -> OxResult<()> {
        super::require_workspace_context()?;
        self.record_audit_for_workspace(user_id, None, action, resource_type, resource_id, details)
            .await
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn record_audit_for_workspace(
        &self,
        user_id: Option<Uuid>,
        affected_workspace_id: Option<Uuid>,
        action: &str,
        resource_type: &str,
        resource_id: Option<&str>,
        details: serde_json::Value,
    ) -> OxResult<()> {
        super::require_workspace_context()?;
        sqlx::query(
            "INSERT INTO audit_log (user_id, action, resource_type, resource_id, details, affected_workspace_id)
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(user_id)
        .bind(action)
        .bind(resource_type)
        .bind(resource_id)
        .bind(&details)
        .bind(affected_workspace_id)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_audit_events(&self, params: CursorParams) -> OxResult<CursorPage<AuditEntry>> {
        let limit = params.effective_limit();

        let rows: Vec<AuditEntry> = if let Some((cursor_ts, cursor_id)) = params.cursor_parts() {
            sqlx::query_as(
                "SELECT id, user_id, workspace_id, affected_workspace_id, action, resource_type, resource_id, details, created_at
                 FROM audit_log
                 WHERE (created_at, id) < ($1, $2)
                 ORDER BY created_at DESC, id DESC
                 LIMIT $3",
            )
            .bind(cursor_ts)
            .bind(cursor_id)
            .bind(limit + 1)
            .fetch_all(&self.pool)
            .await
            .map_err(to_ox_error)?
        } else {
            sqlx::query_as(
                "SELECT id, user_id, workspace_id, affected_workspace_id, action, resource_type, resource_id, details, created_at
                 FROM audit_log
                 ORDER BY created_at DESC, id DESC
                 LIMIT $1",
            )
            .bind(limit + 1)
            .fetch_all(&self.pool)
            .await
            .map_err(to_ox_error)?
        };

        Ok(build_cursor_page(rows, limit, |entry| {
            (entry.created_at, entry.id)
        }))
    }
}
