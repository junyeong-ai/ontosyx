//! Configurable gates for schema deployment + migration.

use async_trait::async_trait;
use uuid::Uuid;

use ox_core::error::OxResult;

use crate::models::{ApprovalComment, ApprovalRequest};

#[async_trait]
pub trait ApprovalStore: Send + Sync {
    /// Create a new approval request.
    async fn create_approval_request(
        &self,
        requester_id: Uuid,
        action_type: &str,
        resource_type: &str,
        resource_id: &str,
        payload: serde_json::Value,
    ) -> OxResult<ApprovalRequest>;

    /// Get a single approval request by ID.
    async fn get_approval_request(&self, id: Uuid) -> OxResult<Option<ApprovalRequest>>;

    /// List pending approvals for the current workspace.
    async fn list_pending_approvals(&self, workspace_id: Uuid) -> OxResult<Vec<ApprovalRequest>>;

    /// Approve or reject an approval request. A non-empty trimmed
    /// `note` is recorded as the first entry on the comment thread
    /// in the same transaction as the row update — both writes land
    /// or both roll back. Returns the created comment when one was
    /// recorded.
    async fn review_approval(
        &self,
        id: Uuid,
        reviewer_id: Uuid,
        approved: bool,
        note: Option<String>,
    ) -> OxResult<Option<ApprovalComment>>;

    /// Bulk variant — apply the same decision to every pending
    /// approval whose id is in `ids`. Returns the count of rows
    /// actually transitioned (rows already terminal are silently
    /// skipped, mirroring single-id semantics). One round-trip
    /// regardless of `ids.len()`. The optional `note` is appended
    /// to *every* transitioned row's comment thread atomically;
    /// either every row + every comment lands, or none do.
    async fn review_approvals(
        &self,
        ids: &[Uuid],
        reviewer_id: Uuid,
        approved: bool,
        note: Option<String>,
    ) -> OxResult<u64>;

    /// Expire old pending approvals past their `expires_at`.
    /// Returns per-workspace counts so the maintenance loop can record
    /// one audit row per affected workspace.
    async fn expire_old_approvals(&self) -> OxResult<Vec<(Uuid, u64)>>;
}
