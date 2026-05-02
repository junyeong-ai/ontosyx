use axum::Json;
use axum::extract::{Path, State};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
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

#[derive(Deserialize, ToSchema)]
pub struct CreateWorkspaceRequest {
    pub name: String,
    pub slug: String,
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateWorkspaceRequest {
    pub name: String,
    #[serde(default)]
    #[schema(value_type = Object)]
    pub settings: serde_json::Value,
}

/// Body for `PUT /workspaces/:id/locale`.
///
/// `primary_locale` must be a BCP 47 tag (lowercase canonical form).
/// Both fallback chains are non-empty ordered lists of BCP 47 tags;
/// `admin_locale_fallback` is what the admin / operator UI walks
/// (typically `["ko", "en"]`), and `llm_locale_fallback` is what
/// the agent / Brain prompts and tool-result contexts walk
/// (typically `["en", "ko"]`). Each is validated at the ox-core
/// layer via `LanguageTag::parse` before hitting the DB, and again
/// at the DB layer by `fn_validate_locale_chain` — malformed values
/// are rejected twice before any row is touched.
#[derive(Deserialize, ToSchema)]
pub struct UpdateWorkspaceLocaleRequest {
    pub primary_locale: String,
    pub admin_locale_fallback: Vec<String>,
    pub llm_locale_fallback: Vec<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct AddMemberRequest {
    pub user_id: Uuid,
    #[serde(default = "default_member_role")]
    pub role: String,
}

fn default_member_role() -> String {
    "member".to_string()
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateMemberRoleRequest {
    pub role: String,
}

#[derive(Serialize, ToSchema)]
pub struct WorkspaceResponse {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub owner_id: Uuid,
    #[schema(value_type = Object)]
    pub settings: serde_json::Value,
    pub primary_locale: String,
    #[schema(value_type = Vec<String>)]
    pub admin_locale_fallback: serde_json::Value,
    #[schema(value_type = Vec<String>)]
    pub llm_locale_fallback: serde_json::Value,
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
            admin_locale_fallback: w.admin_locale_fallback,
            llm_locale_fallback: w.llm_locale_fallback,
            created_at: w.created_at,
        }
    }
}

#[derive(Serialize, ToSchema)]
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

#[derive(Serialize, ToSchema)]
pub struct MemberResponse {
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub role: String,
    pub joined_at: chrono::DateTime<chrono::Utc>,
    pub email: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub picture: Option<String>,
}

impl From<WorkspaceMember> for MemberResponse {
    fn from(m: WorkspaceMember) -> Self {
        Self {
            workspace_id: m.workspace_id,
            user_id: m.user_id,
            role: m.role,
            joined_at: m.joined_at,
            email: m.email,
            name: m.name,
            picture: m.picture,
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
#[utoipa::path(
    post,
    path = "/api/workspaces",
    request_body = CreateWorkspaceRequest,
    responses(
        (status = 200, description = "Workspace created", body = WorkspaceResponse),
        (status = 400, description = "Invalid slug or name"),
        (status = 403, description = "Designer role required"),
    ),
    security(("api_key" = [])),
    tag = "Workspaces",
)]
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
        // Workspace locale policy defaults — single source of truth
        // in `ox_core::{PRIMARY_LOCALE_DEFAULT,
        // ADMIN_LOCALE_FALLBACK_DEFAULT, LLM_LOCALE_FALLBACK_DEFAULT}`.
        // Tunable at runtime via `PUT /api/workspaces/{id}/locale`.
        primary_locale: ox_core::PRIMARY_LOCALE_DEFAULT.to_string(),
        admin_locale_fallback: serde_json::json!(ox_core::ADMIN_LOCALE_FALLBACK_DEFAULT),
        llm_locale_fallback: serde_json::json!(ox_core::LLM_LOCALE_FALLBACK_DEFAULT),
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
#[utoipa::path(
    get,
    path = "/api/workspaces",
    responses(
        (status = 200, description = "Workspaces the caller belongs to", body = Vec<WorkspaceSummaryResponse>),
    ),
    security(("api_key" = [])),
    tag = "Workspaces",
)]
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
#[utoipa::path(
    get,
    path = "/api/workspaces/{id}",
    params(("id" = Uuid, Path, description = "Workspace ID")),
    responses(
        (status = 200, description = "Workspace details", body = WorkspaceResponse),
        (status = 404, description = "Workspace not found"),
    ),
    security(("api_key" = [])),
    tag = "Workspaces",
)]
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

/// Per-request workspace identity surface for the FE. Carries the
/// caller's *active* workspace plus both locale chains the renderer
/// needs — the FE's `useLocaleChain` hook reads this once per
/// workspace switch instead of bolting locale onto `/auth/me`
/// (workspace-level data on a user-level endpoint).
#[derive(Serialize, utoipa::ToSchema)]
pub struct WorkspaceMeResponse {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub role: String,
    /// BCP 47 tag — the workspace's default authoring locale.
    pub primary_locale: String,
    /// Ordered fallback chain the admin / operator UI walks
    /// (e.g. `["ko", "en"]`). FE `localize()` reads this; first
    /// non-empty translation wins.
    pub admin_locale_fallback: Vec<String>,
    /// Ordered fallback chain the agent / Brain prompts and
    /// tool-result contexts walk (e.g. `["en", "ko"]`). Distinct
    /// from `admin_locale_fallback` so a Korean-first admin
    /// surface can pair with an English-first LLM context.
    pub llm_locale_fallback: Vec<String>,
}

/// GET /workspaces/me — return the active workspace context.
///
/// `WorkspaceContext` is set by the middleware after authentication;
/// the handler enriches it with the workspace row's `name`,
/// `primary_locale`, `admin_locale_fallback`, and
/// `llm_locale_fallback` so the FE doesn't make a second
/// round-trip.
#[utoipa::path(
    get,
    path = "/workspaces/me",
    responses(
        (status = 200, description = "Active workspace context", body = WorkspaceMeResponse),
        (status = 404, description = "Workspace row missing", body = inline(crate::openapi::ErrorResponse)),
    ),
    tag = "Workspaces",
    security(("bearer" = [])),
)]
pub(crate) async fn workspace_me(
    State(state): State<AppState>,
    ws_ctx: WorkspaceContext,
) -> Result<Json<ApiResponse<WorkspaceMeResponse>>, AppError> {
    let workspace = state
        .store
        .get_workspace(ws_ctx.workspace_id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Workspace"))?;

    let admin_locale_fallback: Vec<String> =
        serde_json::from_value(workspace.admin_locale_fallback.clone()).map_err(|e| {
            AppError::internal(format!(
                "workspaces.admin_locale_fallback for {} is not a JSON string array: {e}",
                ws_ctx.workspace_id
            ))
        })?;
    let llm_locale_fallback: Vec<String> =
        serde_json::from_value(workspace.llm_locale_fallback.clone()).map_err(|e| {
            AppError::internal(format!(
                "workspaces.llm_locale_fallback for {} is not a JSON string array: {e}",
                ws_ctx.workspace_id
            ))
        })?;

    Ok(ApiResponse::of(WorkspaceMeResponse {
        id: workspace.id,
        name: workspace.name,
        slug: workspace.slug,
        role: ws_ctx.workspace_role.as_str().to_string(),
        primary_locale: workspace.primary_locale,
        admin_locale_fallback,
        llm_locale_fallback,
    }))
}

/// PATCH /workspaces/:id — update workspace name/settings.
#[utoipa::path(
    patch,
    path = "/api/workspaces/{id}",
    params(("id" = Uuid, Path, description = "Workspace ID")),
    request_body = UpdateWorkspaceRequest,
    responses(
        (status = 200, description = "Workspace updated", body = WorkspaceResponse),
        (status = 403, description = "Admin role required"),
        (status = 404, description = "Workspace not found"),
    ),
    security(("api_key" = [])),
    tag = "Workspaces",
)]
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
/// Admins only. Validates the primary locale and every entry of
/// both fallback chains against `LanguageTag::parse` (BCP 47
/// subset) before handing off to the store — the DB CHECK
/// constraints catch any oversight as a final safety net.
#[utoipa::path(
    put,
    path = "/api/workspaces/{id}/locale",
    params(("id" = Uuid, Path, description = "Workspace ID")),
    request_body = UpdateWorkspaceLocaleRequest,
    responses(
        (status = 200, description = "Locale policy updated", body = WorkspaceResponse),
        (status = 400, description = "Invalid BCP 47 tag"),
        (status = 403, description = "Admin role required"),
        (status = 404, description = "Workspace not found"),
    ),
    security(("api_key" = [])),
    tag = "Workspaces",
)]
pub(crate) async fn update_workspace_locale(
    State(state): State<AppState>,
    ws_ctx: WorkspaceContext,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateWorkspaceLocaleRequest>,
) -> Result<Json<ApiResponse<WorkspaceResponse>>, AppError> {
    ws_ctx.require_admin()?;

    let primary = ox_core::LanguageTag::parse(&req.primary_locale)
        .map_err(|e| AppError::bad_request(format!("Invalid primary_locale: {e}")))?;

    let admin_chain = parse_chain(&req.admin_locale_fallback, "admin_locale_fallback")?;
    let llm_chain = parse_chain(&req.llm_locale_fallback, "llm_locale_fallback")?;

    state
        .store
        .update_workspace_locale(id, primary.as_ref(), &admin_chain, &llm_chain)
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

/// Parse a single locale chain — non-empty, every entry a valid
/// BCP 47 tag — into the canonical lowercase JSON array shape the
/// store expects. Reports the offending field via `chain_name` so a
/// failed admin / llm chain is unambiguous in the error.
fn parse_chain(input: &[String], chain_name: &str) -> Result<serde_json::Value, AppError> {
    if input.is_empty() {
        return Err(AppError::bad_request(format!(
            "{chain_name} must contain at least one tag"
        )));
    }
    let mut canonical: Vec<String> = Vec::with_capacity(input.len());
    for (idx, tag) in input.iter().enumerate() {
        let parsed = ox_core::LanguageTag::parse(tag).map_err(|e| {
            AppError::bad_request(format!("Invalid {chain_name}[{idx}] `{tag}`: {e}"))
        })?;
        canonical.push(parsed.to_string());
    }
    Ok(serde_json::Value::Array(
        canonical.into_iter().map(serde_json::Value::String).collect(),
    ))
}

/// DELETE /workspaces/:id — delete a workspace (owner only).
#[utoipa::path(
    delete,
    path = "/api/workspaces/{id}",
    params(("id" = Uuid, Path, description = "Workspace ID")),
    responses(
        (status = 200, description = "Workspace deleted"),
        (status = 400, description = "Cannot delete the default workspace"),
        (status = 403, description = "Only the workspace owner can delete it"),
        (status = 404, description = "Workspace not found"),
    ),
    security(("api_key" = [])),
    tag = "Workspaces",
)]
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
#[utoipa::path(
    post,
    path = "/api/workspaces/{id}/members",
    params(("id" = Uuid, Path, description = "Workspace ID")),
    request_body = AddMemberRequest,
    responses(
        (status = 200, description = "Member added (or role updated on re-add)", body = MemberResponse),
        (status = 400, description = "Invalid role"),
        (status = 403, description = "Admin role required"),
    ),
    security(("api_key" = [])),
    tag = "Workspaces",
)]
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

    let member = state
        .store
        .add_workspace_member(id, req.user_id, &req.role)
        .await
        .map_err(AppError::from)?;

    Ok(ApiResponse::of(member.into()))
}

/// DELETE /workspaces/:id/members/:uid — remove a member.
#[utoipa::path(
    delete,
    path = "/api/workspaces/{id}/members/{uid}",
    params(
        ("id" = Uuid, Path, description = "Workspace ID"),
        ("uid" = Uuid, Path, description = "User ID to remove"),
    ),
    responses(
        (status = 200, description = "Member removed"),
        (status = 400, description = "Cannot remove the workspace owner"),
        (status = 403, description = "Admin role required (or self-removal)"),
        (status = 404, description = "Workspace or member not found"),
    ),
    security(("api_key" = [])),
    tag = "Workspaces",
)]
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
#[utoipa::path(
    patch,
    path = "/api/workspaces/{id}/members/{uid}",
    params(
        ("id" = Uuid, Path, description = "Workspace ID"),
        ("uid" = Uuid, Path, description = "User ID"),
    ),
    request_body = UpdateMemberRoleRequest,
    responses(
        (status = 200, description = "Member role updated", body = MemberResponse),
        (status = 400, description = "Invalid role / cannot change owner role"),
        (status = 403, description = "Admin role required"),
        (status = 404, description = "Workspace or member not found"),
    ),
    security(("api_key" = [])),
    tag = "Workspaces",
)]
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
#[utoipa::path(
    get,
    path = "/api/workspaces/{id}/members",
    params(("id" = Uuid, Path, description = "Workspace ID")),
    responses(
        (status = 200, description = "Members of the workspace", body = Vec<MemberResponse>),
    ),
    security(("api_key" = [])),
    tag = "Workspaces",
)]
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
