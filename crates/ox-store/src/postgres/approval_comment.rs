//! [`ApprovalCommentStore`] — append-only thread of comments on an approval request.

use super::*;

/// Shared SELECT body that joins `users` for the author display
/// name. Every read path uses the same projection so the response
/// shape is identical regardless of entry point.
const SELECT_COMMENT: &str = "
    SELECT
        c.id,
        c.workspace_id,
        c.approval_id,
        c.author_id,
        u.name AS author_name,
        c.body,
        c.created_at
    FROM approval_comments c
    LEFT JOIN users u ON u.id = c.author_id
";

#[async_trait]
impl ApprovalCommentStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_approval_comments(&self, approval_id: Uuid) -> OxResult<Vec<ApprovalComment>> {
        sqlx::query_as(&format!(
            "{SELECT_COMMENT}
             WHERE c.approval_id = $1
             ORDER BY c.created_at ASC, c.id ASC"
        ))
        .bind(approval_id)
        .fetch_all(&self.pool)
        .await
        .map_err(to_ox_error)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn create_approval_comment(
        &self,
        approval_id: Uuid,
        author_id: Uuid,
        body: &str,
    ) -> OxResult<ApprovalComment> {
        sqlx::query_as(
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
        .bind(approval_id)
        .bind(author_id)
        .bind(body)
        .fetch_one(&self.pool)
        .await
        .map_err(to_ox_error)
    }
}
