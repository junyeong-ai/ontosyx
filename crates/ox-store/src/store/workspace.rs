//! Workspace + membership management. Pre-scope tables — not subject
//! to RLS (the auth middleware reads them before `WORKSPACE_ID.scope`
//! wraps the request).

use async_trait::async_trait;
use uuid::Uuid;

use ox_core::error::OxResult;

use crate::models::{Workspace, WorkspaceMember, WorkspaceSummary};

#[async_trait]
pub trait WorkspaceStore: Send + Sync {
    async fn create_workspace(&self, workspace: &Workspace) -> OxResult<()>;
    async fn get_workspace(&self, id: Uuid) -> OxResult<Option<Workspace>>;
    async fn find_workspace_by_slug(&self, slug: &str) -> OxResult<Option<Workspace>>;
    async fn list_user_workspaces(&self, user_id: Uuid) -> OxResult<Vec<WorkspaceSummary>>;
    async fn update_workspace(
        &self,
        id: Uuid,
        name: &str,
        settings: &serde_json::Value,
    ) -> OxResult<()>;
    async fn delete_workspace(&self, id: Uuid) -> OxResult<bool>;

    /// Read the workspace's typed evaluation settings — pulls
    /// the `evaluation` slot off the `workspaces.settings`
    /// JSONB and deserialises into
    /// [`crate::evaluation::WorkspaceEvaluationSettings`]. Missing
    /// slot or malformed payload → platform defaults; the
    /// caller never has to handle the absence path.
    async fn get_evaluation_settings(
        &self,
        workspace_id: Uuid,
    ) -> OxResult<crate::evaluation::WorkspaceEvaluationSettings>;

    /// Persist typed evaluation settings into the workspace's
    /// `settings.evaluation` slot. Other keys on the same JSONB
    /// (`evaluation` is one of several namespaces) are
    /// preserved — this is a partial update, not a wholesale
    /// replace. Validation against
    /// [`crate::evaluation::WorkspaceEvaluationSettings::validate`]
    /// is the caller's responsibility (route boundary).
    async fn update_evaluation_settings(
        &self,
        workspace_id: Uuid,
        settings: &crate::evaluation::WorkspaceEvaluationSettings,
    ) -> OxResult<()>;

    /// Update the workspace's primary locale + both fallback chains.
    /// `primary_locale` must be a BCP 47 tag (ox-core's
    /// `LanguageTag::parse` syntax); both fallback chains must be
    /// non-empty canonical BCP 47 tags. All three are enforced by DB
    /// CHECK constraints.
    async fn update_workspace_locale(
        &self,
        id: Uuid,
        primary_locale: &str,
        admin_locale_fallback: &[String],
        llm_locale_fallback: &[String],
    ) -> OxResult<()>;

    // Membership
    async fn add_workspace_member(
        &self,
        workspace_id: Uuid,
        user_id: Uuid,
        role: &str,
    ) -> OxResult<WorkspaceMember>;
    async fn remove_workspace_member(&self, workspace_id: Uuid, user_id: Uuid) -> OxResult<bool>;
    async fn update_member_role(
        &self,
        workspace_id: Uuid,
        user_id: Uuid,
        role: &str,
    ) -> OxResult<()>;
    async fn get_member_role(&self, workspace_id: Uuid, user_id: Uuid) -> OxResult<Option<String>>;
    async fn list_workspace_members(&self, workspace_id: Uuid) -> OxResult<Vec<WorkspaceMember>>;

    /// Get user's default workspace (first workspace they belong to, or the "default" slug).
    async fn find_default_workspace(&self, user_id: Uuid) -> OxResult<Option<Workspace>>;

    /// Every workspace id known to the cluster. Used by
    /// system-bypass cron jobs that fan out per-tenant work — the
    /// per-workspace bodies run inside `WORKSPACE_ID.scope(id, …)`
    /// so RLS lands on the right tenant.
    async fn list_workspace_ids(&self) -> OxResult<Vec<Uuid>>;
}
