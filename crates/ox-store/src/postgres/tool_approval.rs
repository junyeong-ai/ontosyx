//! [`ToolApprovalStore`] — per-tool-call approval decisions (allow/deny with optional input override).

use super::*;

#[async_trait]
impl ToolApprovalStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn create_tool_approval(&self, a: &ToolApproval) -> OxResult<()> {
        super::require_workspace_context()?;
        sqlx::query(
            "INSERT INTO tool_approvals
             (session_id, tool_call_id, approved, reason, modified_input, user_id)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (session_id, tool_call_id) DO UPDATE
             SET approved = EXCLUDED.approved,
                 reason = EXCLUDED.reason,
                 modified_input = EXCLUDED.modified_input,
                 user_id = EXCLUDED.user_id",
        )
        .bind(a.session_id)
        .bind(&a.tool_call_id)
        .bind(a.approved)
        .bind(&a.reason)
        .bind(&a.modified_input)
        .bind(&a.user_id)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_tool_approval(
        &self,
        session_id: Uuid,
        tool_call_id: &str,
    ) -> OxResult<Option<ToolApproval>> {
        sqlx::query_as("SELECT * FROM tool_approvals WHERE session_id = $1 AND tool_call_id = $2")
            .bind(session_id)
            .bind(tool_call_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(to_ox_error)
    }
}
