use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use ox_store::{Workspace, WorkspaceMember, WorkspaceSummary};

use crate::error::AppError;
use crate::principal::Principal;
use crate::state::AppState;
use crate::workspace::{
    WorkspaceContext, WorkspaceRole, DEFAULT_WORKSPACE_SLUG, ASSIGNABLE_WORKSPACE_ROLES,
};

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct CreateWorkspaceRequest {
    pub name: String,
    pub slug: String,
}

#[derive(Deserialize)]
pub struct UpdateWorkspaceRequest {
    pub name: String,
    #[serde(default)]
    pub settings: serde_json::Value,
}

#[derive(Deserialize)]
pub struct AddMemberRequest {
    pub user_id: Uuid,
    #[serde(default = "default_member_role")]
    pub role: String,
}

fn default_member_role() -> String {
    "member".to_string()
}

#[derive(Deserialize)]
pub struct UpdateMemberRoleRequest {
    pub role: String,
}

#[derive(Serialize)]
pub struct WorkspaceResponse {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub owner_id: Uuid,
    pub settings: serde_json::Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl From<Workspace> for WorkspaceResponse {
    fn from(w: Workspace) -> Self {
        Self {
            id: w.id,
            name: w.name,
            slug: w.slug,
            owner_id: w.owner_id,
            settings: w.settings,
            created_at: w.created_at,
        }
    }
}

#[derive(Serialize)]
pub struct WorkspaceSummaryResponse {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub owner_id: Uuid,
    pub role: String,
    pub member_count: i64,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl From<WorkspaceSummary> for WorkspaceSummaryResponse {
    fn from(w: WorkspaceSummary) -> Self {
        Self {
            id: w.id,
            name: w.name,
            slug: w.slug,
            owner_id: w.owner_id,
            role: w.role,
            member_count: w.member_count,
            created_at: w.created_at,
        }
    }
}

#[derive(Serialize)]
pub struct MemberResponse {
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub role: String,
    pub joined_at: chrono::DateTime<chrono::Utc>,
}

impl From<WorkspaceMember> for MemberResponse {
    fn from(m: WorkspaceMember) -> Self {
        Self {
            workspace_id: m.workspace_id,
            user_id: m.user_id,
            role: m.role,
            joined_at: m.joined_at,
        }
    }
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// Resolve user UUID, falling back to default workspace owner for system users.
async fn resolve_user_id(principal: &Principal, state: &AppState) -> Result<Uuid, AppError> {
    if principal.is_system() {
        // System/API-key users: use the default workspace owner as proxy identity
        let ws = state
            .store
            .get_workspace_by_slug(DEFAULT_WORKSPACE_SLUG)
            .await
            .map_err(AppError::from)?
            .ok_or_else(|| AppError::internal("Default workspace not found"))?;
        Ok(ws.owner_id)
    } else {
        principal.user_uuid()
    }
}

/// POST /workspaces — create a new workspace.
pub async fn create_workspace(
    State(state): State<AppState>,
    principal: Principal,
    Json(req): Json<CreateWorkspaceRequest>,
) -> Result<Json<WorkspaceResponse>, AppError> {
    principal.require_designer()?;

    let user_id = resolve_user_id(&principal, &state).await?;

    // Validate slug
    if req.slug.is_empty() || req.slug.len() > 100 {
        return Err(AppError::bad_request("Slug must be 1-100 characters"));
    }
    if !req.slug.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err(AppError::bad_request(
            "Slug may only contain alphanumeric characters, hyphens, and underscores",
        ));
    }

    let workspace = Workspace {
        id: Uuid::new_v4(),
        name: req.name,
        slug: req.slug,
        owner_id: user_id,
        settings: serde_json::json!({}),
        created_at: chrono::Utc::now(),
    };

    state.store.create_workspace(&workspace).await.map_err(AppError::from)?;

    // Auto-add creator as owner
    state
        .store
        .add_workspace_member(workspace.id, user_id, "owner")
        .await
        .map_err(AppError::from)?;

    tracing::info!(
        workspace_id = %workspace.id,
        slug = %workspace.slug,
        "Workspace created"
    );

    Ok(Json(workspace.into()))
}

/// GET /workspaces — list workspaces the current user belongs to.
pub async fn list_workspaces(
    State(state): State<AppState>,
    principal: Principal,
) -> Result<Json<Vec<WorkspaceSummaryResponse>>, AppError> {
    let user_id = resolve_user_id(&principal, &state).await?;

    let workspaces = state
        .store
        .list_user_workspaces(user_id)
        .await
        .map_err(AppError::from)?;

    Ok(Json(workspaces.into_iter().map(Into::into).collect()))
}

/// GET /workspaces/:id — get workspace details.
pub async fn get_workspace(
    State(state): State<AppState>,
    _ws_ctx: WorkspaceContext,
    Path(id): Path<Uuid>,
) -> Result<Json<WorkspaceResponse>, AppError> {
    let workspace = state
        .store
        .get_workspace(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Workspace"))?;

    Ok(Json(workspace.into()))
}

/// PATCH /workspaces/:id — update workspace name/settings.
pub async fn update_workspace(
    State(state): State<AppState>,
    ws_ctx: WorkspaceContext,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateWorkspaceRequest>,
) -> Result<Json<WorkspaceResponse>, AppError> {
    ws_ctx.require_admin()?;

    let settings = if req.settings.is_null() {
        serde_json::json!({})
    } else {
        req.settings
    };

    state
        .store
        .update_workspace(id, &req.name, &settings)
        .await
        .map_err(AppError::from)?;

    let workspace = state
        .store
        .get_workspace(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Workspace"))?;

    Ok(Json(workspace.into()))
}

/// DELETE /workspaces/:id — delete a workspace (owner only).
pub async fn delete_workspace(
    State(state): State<AppState>,
    ws_ctx: WorkspaceContext,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    if ws_ctx.workspace_role != WorkspaceRole::Owner {
        return Err(AppError::forbidden("Only the workspace owner can delete it"));
    }

    // Prevent deleting the "default" workspace
    let workspace = state
        .store
        .get_workspace(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Workspace"))?;

    if workspace.slug == DEFAULT_WORKSPACE_SLUG {
        return Err(AppError::bad_request("Cannot delete the default workspace"));
    }

    state.store.delete_workspace(id).await.map_err(AppError::from)?;

    tracing::info!(workspace_id = %id, "Workspace deleted");
    Ok(Json(serde_json::json!({"deleted": true})))
}

// ---------------------------------------------------------------------------
// Member management
// ---------------------------------------------------------------------------

/// POST /workspaces/:id/members — add a member.
pub async fn add_member(
    State(state): State<AppState>,
    ws_ctx: WorkspaceContext,
    Path(id): Path<Uuid>,
    Json(req): Json<AddMemberRequest>,
) -> Result<Json<MemberResponse>, AppError> {
    ws_ctx.require_admin()?;

    // Validate role
    if !ASSIGNABLE_WORKSPACE_ROLES.contains(&req.role.as_str()) {
        return Err(AppError::bad_request(format!(
            "Invalid role '{}'. Assignable roles: {:?}",
            req.role, ASSIGNABLE_WORKSPACE_ROLES
        )));
    }

    state
        .store
        .add_workspace_member(id, req.user_id, &req.role)
        .await
        .map_err(AppError::from)?;

    Ok(Json(MemberResponse {
        workspace_id: id,
        user_id: req.user_id,
        role: req.role,
        joined_at: chrono::Utc::now(),
    }))
}

/// DELETE /workspaces/:id/members/:uid — remove a member.
pub async fn remove_member(
    State(state): State<AppState>,
    ws_ctx: WorkspaceContext,
    Path((id, uid)): Path<(Uuid, Uuid)>,
    principal: Principal,
) -> Result<Json<serde_json::Value>, AppError> {
    let caller_id = Uuid::parse_str(&principal.id)
        .map_err(|_| AppError::unauthorized("Invalid user ID"))?;

    // Allow self-removal, or require admin
    if uid != caller_id {
        ws_ctx.require_admin()?;
    }

    // Prevent removing the workspace owner
    let workspace = state
        .store
        .get_workspace(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Workspace"))?;

    if workspace.owner_id == uid {
        return Err(AppError::bad_request(
            "Cannot remove the workspace owner. Transfer ownership first.",
        ));
    }

    let removed = state
        .store
        .remove_workspace_member(id, uid)
        .await
        .map_err(AppError::from)?;

    if !removed {
        return Err(AppError::not_found("Member"));
    }

    Ok(Json(serde_json::json!({"removed": true})))
}

/// PATCH /workspaces/:id/members/:uid — update member role.
pub async fn update_member_role(
    State(state): State<AppState>,
    ws_ctx: WorkspaceContext,
    Path((id, uid)): Path<(Uuid, Uuid)>,
    Json(req): Json<UpdateMemberRoleRequest>,
) -> Result<Json<MemberResponse>, AppError> {
    ws_ctx.require_admin()?;

    if !ASSIGNABLE_WORKSPACE_ROLES.contains(&req.role.as_str()) {
        return Err(AppError::bad_request(format!(
            "Invalid role '{}'. Assignable roles: {:?}",
            req.role, ASSIGNABLE_WORKSPACE_ROLES
        )));
    }

    // Cannot change owner role
    let workspace = state
        .store
        .get_workspace(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Workspace"))?;

    if workspace.owner_id == uid {
        return Err(AppError::bad_request(
            "Cannot change the workspace owner's role",
        ));
    }

    state
        .store
        .update_member_role(id, uid, &req.role)
        .await
        .map_err(AppError::from)?;

    // Fetch updated member info
    let members = state.store.list_workspace_members(id).await.map_err(AppError::from)?;
    let member = members
        .into_iter()
        .find(|m| m.user_id == uid)
        .ok_or_else(|| AppError::not_found("Member"))?;

    Ok(Json(member.into()))
}

/// GET /workspaces/:id/members — list workspace members.
pub async fn list_members(
    State(state): State<AppState>,
    _ws_ctx: WorkspaceContext,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<MemberResponse>>, AppError> {
    let members = state
        .store
        .list_workspace_members(id)
        .await
        .map_err(AppError::from)?;

    Ok(Json(members.into_iter().map(Into::into).collect()))
}
