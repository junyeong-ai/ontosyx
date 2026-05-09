//! Append-only event log for enterprise governance.

use async_trait::async_trait;
use uuid::Uuid;

use ox_core::error::OxResult;

use crate::models::AuditEntry;

use super::{CursorPage, CursorParams};

#[async_trait]
pub trait AuditStore: Send + Sync {
    /// Record an audit event (append-only). The current workspace is
    /// inferred from the `WORKSPACE_ID` task-local. To attribute a
    /// system-bypass action to a specific workspace, use
    /// [`record_audit_for_workspace`].
    async fn record_audit(
        &self,
        user_id: Option<Uuid>,
        action: &str,
        resource_type: &str,
        resource_id: Option<&str>,
        details: serde_json::Value,
    ) -> OxResult<()>;

    /// Record an audit event whose target workspace differs from the
    /// caller's. Used by SYSTEM_BYPASS maintenance tasks so workspace
    /// admins can later see which system actions touched their data.
    ///
    /// `affected_workspace_id` is stored in `audit_log.affected_workspace_id`.
    /// When `None` it falls back to the same
    /// behaviour as [`record_audit`].
    async fn record_audit_for_workspace(
        &self,
        user_id: Option<Uuid>,
        affected_workspace_id: Option<Uuid>,
        action: &str,
        resource_type: &str,
        resource_id: Option<&str>,
        details: serde_json::Value,
    ) -> OxResult<()>;

    /// List audit events with cursor pagination.
    async fn list_audit_events(&self, params: CursorParams) -> OxResult<CursorPage<AuditEntry>>;
}
