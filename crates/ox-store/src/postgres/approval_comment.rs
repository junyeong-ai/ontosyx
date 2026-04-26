//! [`ApprovalCommentStore`] — append-only thread of comments on an approval request.

use super::*;

#[async_trait]
impl ApprovalCommentStore for PostgresStore {
    #[tracing::instrument(level = "debug", skip_all)]
    async fn list_approval_comments(&self, approval_id: Uuid) -> OxResult<Vec<ApprovalComment>> {
        sqlx::query_as(
            "SELECT * FROM approval_comments
             WHERE approval_id = $1
             ORDER BY created_at ASC, id ASC",
        )
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
        // RLS guarantees the row can only land in the caller's
        // workspace; the FK to approval_requests guarantees the
        // parent exists. The CHECK constraint catches whitespace-
        // only bodies — callers should trim first to avoid the
        // database round-trip on obvious empty input.
        sqlx::query_as(
            "INSERT INTO approval_comments (approval_id, author_id, body)
             VALUES ($1, $2, $3)
             RETURNING *",
        )
        .bind(approval_id)
        .bind(author_id)
        .bind(body)
        .fetch_one(&self.pool)
        .await
        .map_err(to_ox_error)
    }
}
