//! Comment thread attached to an approval request.

use async_trait::async_trait;
use uuid::Uuid;

use ox_core::error::OxResult;

use crate::models::ApprovalComment;

#[async_trait]
pub trait ApprovalCommentStore: Send + Sync {
    /// List every comment attached to an approval, oldest first.
    async fn list_approval_comments(&self, approval_id: Uuid) -> OxResult<Vec<ApprovalComment>>;

    /// Append a comment to an approval thread.
    async fn create_approval_comment(
        &self,
        approval_id: Uuid,
        author_id: Uuid,
        body: &str,
    ) -> OxResult<ApprovalComment>;
}
