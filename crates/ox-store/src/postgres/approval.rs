//! [`ApprovalStore`] — pending approval requests queue (time-boxed; 7-day default).

use super::*;

/// Shared SELECT body that joins `users` twice for the requester and
/// reviewer display names. Every read path uses the same projection
/// so the response shape is identical regardless of entry point.
const SELECT_APPROVAL: &str = "
    SELECT
        a.id,
        a.workspace_id,
        a.requester_id,
        ru.name           AS requester_name,
        a.action_type,
        a.resource_type,
        a.resource_id,
        a.payload,
        a.status,
        a.reviewer_id,
        rv.name           AS reviewer_name,
        a.reviewed_at,
        a.expires_at,
        a.created_at
    FROM approval_requests a
    LEFT JOIN users ru ON ru.id = a.requester_id
    LEFT JOIN users rv ON rv.id = a.reviewer_id
";

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
        super::require_workspace_context()?;
        let inserted_id: (Uuid,) = sqlx::query_as(
            "INSERT INTO approval_requests
             (requester_id, action_type, resource_type, resource_id, payload)
             VALUES ($1, $2, $3, $4, $5)
             RETURNING id",
        )
        .bind(requester_id)
        .bind(action_type)
        .bind(resource_type)
        .bind(resource_id)
        .bind(&payload)
        .fetch_one(&self.pool)
        .await
        .map_err(to_ox_error)?;

        sqlx::query_as(&format!("{SELECT_APPROVAL} WHERE a.id = $1"))
            .bind(inserted_id.0)
            .fetch_one(&self.pool)
            .await
            .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn get_approval_request(&self, id: Uuid) -> OxResult<Option<ApprovalRequest>> {
        sqlx::query_as(&format!("{SELECT_APPROVAL} WHERE a.id = $1"))
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_pending_approvals(&self, workspace_id: Uuid) -> OxResult<Vec<ApprovalRequest>> {
        sqlx::query_as(&format!(
            "{SELECT_APPROVAL}
             WHERE a.workspace_id = $1 AND a.status = 'pending' AND a.expires_at > NOW()
             ORDER BY a.created_at DESC"
        ))
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
        note: Option<String>,
    ) -> OxResult<Option<ApprovalComment>> {
        super::require_workspace_context()?;
        // Atomic: the row update and the first-comment insert either
        // both land or both roll back. The reviewer's rationale lives
        // in the thread alone — the row carries the decision metadata
        // (status, reviewer, timestamp) and nothing else.
        let trimmed = note
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let mut tx = self.pool.begin().await.map_err(to_ox_error)?;

        let status = if approved { "approved" } else { "rejected" };
        let result = sqlx::query(
            "UPDATE approval_requests
             SET status = $1, reviewer_id = $2, reviewed_at = NOW()
             WHERE id = $3 AND status = 'pending'",
        )
        .bind(status)
        .bind(reviewer_id)
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(to_ox_error)?;

        if result.rows_affected() == 0 {
            return Err(OxError::NotFound {
                entity: format!("pending approval request {id}"),
            });
        }

        let comment = match trimmed {
            Some(body) => Some(
                sqlx::query_as::<_, ApprovalComment>(
                    "WITH inserted AS (
                         INSERT INTO approval_comments (approval_id, author_id, body)
                         VALUES ($1, $2, $3)
                         RETURNING *
                     )
                     SELECT
                         i.id, i.workspace_id, i.approval_id, i.author_id,
                         u.name AS author_name,
                         i.body, i.created_at
                     FROM inserted i
                     LEFT JOIN users u ON u.id = i.author_id",
                )
                .bind(id)
                .bind(reviewer_id)
                .bind(body)
                .fetch_one(&mut *tx)
                .await
                .map_err(to_ox_error)?,
            ),
            None => None,
        };

        tx.commit().await.map_err(to_ox_error)?;
        Ok(comment)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn review_approvals(
        &self,
        ids: &[Uuid],
        reviewer_id: Uuid,
        approved: bool,
        note: Option<String>,
    ) -> OxResult<u64> {
        super::require_workspace_context()?;
        if ids.is_empty() {
            return Ok(0);
        }
        let trimmed = note
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let mut tx = self.pool.begin().await.map_err(to_ox_error)?;

        let status = if approved { "approved" } else { "rejected" };
        // `RETURNING id` lets us know exactly which rows transitioned —
        // the cohort may overlap rows already in a terminal state, so
        // the comment-insert below targets only those that just moved.
        let transitioned: Vec<(Uuid,)> = sqlx::query_as(
            "UPDATE approval_requests
             SET status = $1, reviewer_id = $2, reviewed_at = NOW()
             WHERE id = ANY($3) AND status = 'pending'
             RETURNING id",
        )
        .bind(status)
        .bind(reviewer_id)
        .bind(ids)
        .fetch_all(&mut *tx)
        .await
        .map_err(to_ox_error)?;

        if let Some(body) = &trimmed {
            // Insert one comment per transitioned row in a single
            // statement — UNNEST broadcasts the body + author over
            // the id list. No comment row when `note` was empty/
            // missing, matching the single-id semantics.
            sqlx::query(
                "INSERT INTO approval_comments (approval_id, author_id, body)
                 SELECT id, $2, $3 FROM UNNEST($1::uuid[]) AS t(id)",
            )
            .bind(transitioned.iter().map(|(id,)| *id).collect::<Vec<_>>())
            .bind(reviewer_id)
            .bind(body)
            .execute(&mut *tx)
            .await
            .map_err(to_ox_error)?;
        }

        tx.commit().await.map_err(to_ox_error)?;
        Ok(transitioned.len() as u64)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn expire_old_approvals(&self) -> OxResult<Vec<(Uuid, u64)>> {
        super::require_workspace_context()?;
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
