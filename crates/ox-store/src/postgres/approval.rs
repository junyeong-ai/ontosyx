//! [`ApprovalStore`] — pending approval requests queue (time-boxed; 7-day default).

use super::*;

#[async_trait]
impl ApprovalStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn create_approval_request(
        &self,
        requester_id: Uuid,
        action_type: &str,
        resource_type: &str,
        resource_id: &str,
        payload: serde_json::Value,
    ) -> OxResult<ApprovalRequest> {
        sqlx::query_as(
            "INSERT INTO approval_requests
             (requester_id, action_type, resource_type, resource_id, payload)
             VALUES ($1, $2, $3, $4, $5)
             RETURNING *",
        )
        .bind(requester_id)
        .bind(action_type)
        .bind(resource_type)
        .bind(resource_id)
        .bind(&payload)
        .fetch_one(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_approval_request(&self, id: Uuid) -> OxResult<Option<ApprovalRequest>> {
        sqlx::query_as("SELECT * FROM approval_requests WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_pending_approvals(&self, workspace_id: Uuid) -> OxResult<Vec<ApprovalRequest>> {
        sqlx::query_as(
            "SELECT * FROM approval_requests
             WHERE workspace_id = $1 AND status = 'pending' AND expires_at > NOW()
             ORDER BY created_at DESC",
        )
        .bind(workspace_id)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn review_approval(
        &self,
        id: Uuid,
        reviewer_id: Uuid,
        approved: bool,
        notes: Option<&str>,
    ) -> OxResult<()> {
        let status = if approved { "approved" } else { "rejected" };
        let result = sqlx::query(
            "UPDATE approval_requests
             SET status = $1, reviewer_id = $2, review_notes = $3, reviewed_at = NOW()
             WHERE id = $4 AND status = 'pending'",
        )
        .bind(status)
        .bind(reviewer_id)
        .bind(notes)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(to_ox_error)?;

        if result.rows_affected() == 0 {
            return Err(OxError::NotFound {
                entity: format!("pending approval request {id}"),
            });
        }
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn expire_old_approvals(&self) -> OxResult<Vec<(Uuid, u64)>> {
        // Strict `<` so a request whose `expires_at == NOW()` is still
        // valid for its last clock tick — matches the share-token
        // semantics in `get_dashboard_by_share_token`.
        let rows: Vec<(Uuid, i64)> = sqlx::query_as(
            "WITH affected AS (
                 UPDATE approval_requests
                 SET status = 'expired'
                 WHERE status = 'pending' AND expires_at < NOW()
                 RETURNING workspace_id
             )
             SELECT workspace_id, COUNT(*)::bigint
             FROM affected
             GROUP BY workspace_id",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)?;
        Ok(rows.into_iter().map(|(ws, n)| (ws, n as u64)).collect())
    }
}
