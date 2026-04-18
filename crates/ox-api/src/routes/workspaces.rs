use axum::Json;
use axum::extract::{Path, State};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use ox_store::{Workspace, WorkspaceMember, WorkspaceSummary};

use crate::error::AppError;
use crate::principal::Principal;
use crate::response::ApiResponse;
use crate::state::AppState;
use crate::workspace::{
    ASSIGNABLE_WORKSPACE_ROLES, DEFAULT_WORKSPACE_SLUG, WorkspaceContext, WorkspaceRole,
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

/// Body for `PUT /workspaces/:id/locale`.
///
/// `primary_locale` must be a BCP 47 tag (lowercase canonical form).
/// `locale_fallback` is a non-empty ordered list of BCP 47 tags used by
/// `LocalizedText::resolve`. Both are validated at the ox-core layer via
/// `LanguageTag::parse` before hitting the DB, and again at the DB layer
/// by `fn_validate_locale_chain` — a malformed value is rejected twice
/// before any row is touched.
#[derive(Deserialize)]
pub struct UpdateWorkspaceLocaleRequest {
    pub primary_locale: String,
    pub locale_fallback: Vec<String>,
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
    pub primary_locale: String,
    pub locale_fallback: serde_json::Value,
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
            primary_locale: w.primary_locale,
            locale_fallback: w.locale_fallback,
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

/// Resolve user UUID, falling back to default workspace owner for
/// machine principals (system tasks + API keys).
async fn resolve_user_id(principal: &Principal, state: &AppState) -> Result<Uuid, AppError> {
    if principal.is_machine() {
        // Machine principals: use the default workspace owner as proxy identity
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
pub(crate) async fn create_workspace(
    State(state): State<AppState>,
    principal: Principal,
    Json(req): Json<CreateWorkspaceRequest>,
) -> Result<Json<ApiResponse<WorkspaceResponse>>, AppError> {
    principal.require_designer()?;

    let user_id = resolve_user_id(&principal, &state).await?;

    // Validate slug
    if req.slug.is_empty() || req.slug.len() > 100 {
        return Err(AppError::bad_request("Slug must be 1-100 characters"));
    }
    if !req
        .slug
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
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
        // Defaults for Phase A: Korean-first, fall back to English. Both are
        // runtime-tunable via PATCH /api/workspaces/{id}/locale once the
        // corresponding endpoint is wired.
        primary_locale: "ko".to_string(),
        locale_fallback: serde_json::json!(["ko", "en"]),
    };

    state
        .store
        .create_workspace(&workspace)
        .await
        .map_err(AppError::from)?;

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

    Ok(ApiResponse::of(workspace.into()))
}

/// GET /workspaces — list workspaces the current user belongs to.
pub(crate) async fn list_workspaces(
    State(state): State<AppState>,
    principal: Principal,
) -> Result<Json<ApiResponse<Vec<WorkspaceSummaryResponse>>>, AppError> {
    let user_id = resolve_user_id(&principal, &state).await?;

    let workspaces = state
        .store
        .list_user_workspaces(user_id)
        .await
        .map_err(AppError::from)?;

    Ok(ApiResponse::of(
        workspaces.into_iter().map(Into::into).collect(),
    ))
}

/// GET /workspaces/:id — get workspace details.
pub(crate) async fn get_workspace(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<WorkspaceResponse>>, AppError> {
    let workspace = state
        .store
        .get_workspace(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Workspace"))?;

    Ok(ApiResponse::of(workspace.into()))
}

/// PATCH /workspaces/:id — update workspace name/settings.
pub(crate) async fn update_workspace(
    State(state): State<AppState>,
    ws_ctx: WorkspaceContext,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateWorkspaceRequest>,
) -> Result<Json<ApiResponse<WorkspaceResponse>>, AppError> {
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

    Ok(ApiResponse::of(workspace.into()))
}

/// PUT /workspaces/:id/locale — update the workspace's locale policy.
///
/// Admins only. Validates both the primary locale and every fallback
/// entry against `LanguageTag::parse` (BCP 47 subset) before handing
/// off to the store — the DB CHECK constraints catch any oversight as
/// a final safety net.
pub(crate) async fn update_workspace_locale(
    State(state): State<AppState>,
    ws_ctx: WorkspaceContext,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateWorkspaceLocaleRequest>,
) -> Result<Json<ApiResponse<WorkspaceResponse>>, AppError> {
    ws_ctx.require_admin()?;

    // Validate primary locale shape.
    let primary = ox_core::LanguageTag::parse(&req.primary_locale)
        .map_err(|e| AppError::bad_request(format!("Invalid primary_locale: {e}")))?;

    // Fallback chain must be non-empty and every entry must parse.
    if req.locale_fallback.is_empty() {
        return Err(AppError::bad_request(
            "locale_fallback must contain at least one tag",
        ));
    }
    let mut fallback_canonical: Vec<String> = Vec::with_capacity(req.locale_fallback.len());
    for (idx, tag) in req.locale_fallback.iter().enumerate() {
        let parsed = ox_core::LanguageTag::parse(tag).map_err(|e| {
            AppError::bad_request(format!("Invalid locale_fallback[{idx}] `{tag}`: {e}"))
        })?;
        fallback_canonical.push(parsed.to_string());
    }

    let fallback_json = serde_json::Value::Array(
        fallback_canonical
            .into_iter()
            .map(serde_json::Value::String)
            .collect(),
    );

    state
        .store
        .update_workspace_locale(id, primary.as_ref(), &fallback_json)
        .await
        .map_err(AppError::from)?;

    let workspace = state
        .store
        .get_workspace(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Workspace"))?;

    tracing::info!(workspace_id = %id, primary_locale = %req.primary_locale, "Workspace locale updated");
    Ok(ApiResponse::of(workspace.into()))
}

/// DELETE /workspaces/:id — delete a workspace (owner only).
pub(crate) async fn delete_workspace(
    State(state): State<AppState>,
    ws_ctx: WorkspaceContext,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    if ws_ctx.workspace_role != WorkspaceRole::Owner {
        return Err(AppError::forbidden(
            "Only the workspace owner can delete it",
        ));
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

    state
        .store
        .delete_workspace(id)
        .await
        .map_err(AppError::from)?;

    tracing::info!(workspace_id = %id, "Workspace deleted");
    Ok(ApiResponse::of(serde_json::json!({"deleted": true})))
}

// ---------------------------------------------------------------------------
// Member management
// ---------------------------------------------------------------------------

/// POST /workspaces/:id/members — add a member.
pub(crate) async fn add_member(
    State(state): State<AppState>,
    ws_ctx: WorkspaceContext,
    Path(id): Path<Uuid>,
    Json(req): Json<AddMemberRequest>,
) -> Result<Json<ApiResponse<MemberResponse>>, AppError> {
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

    Ok(ApiResponse::of(MemberResponse {
        workspace_id: id,
        user_id: req.user_id,
        role: req.role,
        joined_at: chrono::Utc::now(),
    }))
}

/// DELETE /workspaces/:id/members/:uid — remove a member.
pub(crate) async fn remove_member(
    State(state): State<AppState>,
    ws_ctx: WorkspaceContext,
    Path((id, uid)): Path<(Uuid, Uuid)>,
    principal: Principal,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    let caller_id =
        Uuid::parse_str(&principal.id).map_err(|_| AppError::unauthorized("Invalid user ID"))?;

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

    Ok(ApiResponse::of(serde_json::json!({"removed": true})))
}

/// PATCH /workspaces/:id/members/:uid — update member role.
pub(crate) async fn update_member_role(
    State(state): State<AppState>,
    ws_ctx: WorkspaceContext,
    Path((id, uid)): Path<(Uuid, Uuid)>,
    Json(req): Json<UpdateMemberRoleRequest>,
) -> Result<Json<ApiResponse<MemberResponse>>, AppError> {
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
    let members = state
        .store
        .list_workspace_members(id)
        .await
        .map_err(AppError::from)?;
    let member = members
        .into_iter()
        .find(|m| m.user_id == uid)
        .ok_or_else(|| AppError::not_found("Member"))?;

    Ok(ApiResponse::of(member.into()))
}

/// GET /workspaces/:id/members — list workspace members.
pub(crate) async fn list_members(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<Vec<MemberResponse>>>, AppError> {
    let members = state
        .store
        .list_workspace_members(id)
        .await
        .map_err(AppError::from)?;

    Ok(ApiResponse::of(
        members.into_iter().map(Into::into).collect(),
    ))
}
