use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use chrono::Utc;
use serde::Deserialize;
use uuid::Uuid;

use ox_store::AclPolicy;

use crate::error::AppError;
use crate::principal::Principal;
use crate::state::AppState;
use crate::workspace::WorkspaceContext;

// ---------------------------------------------------------------------------
// Request / Query types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct CreatePolicyRequest {
    pub name: String,
    pub description: Option<String>,
    pub subject_type: String,
    pub subject_value: String,
    pub resource_type: String,
    pub resource_value: Option<String>,
    pub action: String,
    pub properties: Option<Vec<String>>,
    pub mask_pattern: Option<String>,
    pub priority: Option<i32>,
}

#[derive(Deserialize)]
pub struct UpdatePolicyRequest {
    pub name: Option<String>,
    pub action: Option<String>,
    pub properties: Option<Vec<String>>,
    pub mask_pattern: Option<String>,
    pub priority: Option<i32>,
    pub is_active: Option<bool>,
}

#[derive(Deserialize)]
pub struct ListPoliciesParams {
    pub subject_type: Option<String>,
    pub resource_value: Option<String>,
}

// ---------------------------------------------------------------------------
// POST /api/acl/policies — create ACL policy
// ---------------------------------------------------------------------------

pub(crate) async fn create_policy(
    State(state): State<AppState>,
    principal: Principal,
    ws: WorkspaceContext,
    Json(req): Json<CreatePolicyRequest>,
) -> Result<(StatusCode, Json<AclPolicy>), AppError> {
    ws.require_admin()?;

    let policy = AclPolicy {
        id: Uuid::new_v4(),
        workspace_id: ws.workspace_id,
        name: req.name,
        description: req.description,
        subject_type: req.subject_type,
        subject_value: req.subject_value,
        resource_type: req.resource_type,
        resource_value: req.resource_value,
        action: req.action,
        properties: req.properties,
        mask_pattern: req.mask_pattern,
        priority: req.priority.unwrap_or(0),
        is_active: true,
        created_by: principal.user_uuid().ok(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    state
        .store
        .create_acl_policy(&policy)
        .await
        .map_err(AppError::from)?;

    Ok((StatusCode::CREATED, Json(policy)))
}

// ---------------------------------------------------------------------------
// GET /api/acl/policies — list active policies
// ---------------------------------------------------------------------------

pub(crate) async fn list_policies(
    State(state): State<AppState>,
    _principal: Principal,
    ws: WorkspaceContext,
    Query(params): Query<ListPoliciesParams>,
) -> Result<Json<Vec<AclPolicy>>, AppError> {
    ws.require_admin()?;
    let policies = state
        .store
        .list_acl_policies(
            params.subject_type.as_deref(),
            params.resource_value.as_deref(),
        )
        .await
        .map_err(AppError::from)?;

    Ok(Json(policies))
}

// ---------------------------------------------------------------------------
// GET /api/acl/policies/:id — get single policy
// ---------------------------------------------------------------------------

pub(crate) async fn get_policy(
    State(state): State<AppState>,
    _principal: Principal,
    ws: WorkspaceContext,
    Path(id): Path<Uuid>,
) -> Result<Json<AclPolicy>, AppError> {
    ws.require_admin()?;
    let policy = state
        .store
        .get_acl_policy(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("ACL policy"))?;

    Ok(Json(policy))
}

// ---------------------------------------------------------------------------
// PATCH /api/acl/policies/:id — update policy
// ---------------------------------------------------------------------------

pub(crate) async fn update_policy(
    State(state): State<AppState>,
    ws: WorkspaceContext,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdatePolicyRequest>,
) -> Result<Json<AclPolicy>, AppError> {
    ws.require_admin()?;

    let existing = state
        .store
        .get_acl_policy(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("ACL policy"))?;

    let name = req.name.unwrap_or(existing.name);
    let action = req.action.unwrap_or(existing.action);
    let priority = req.priority.unwrap_or(existing.priority);
    let is_active = req.is_active.unwrap_or(existing.is_active);

    // For optional fields, use the request value if provided, else keep existing
    let properties = if req.properties.is_some() {
        req.properties.as_deref()
    } else {
        existing.properties.as_deref()
    };

    let mask_pattern = if req.mask_pattern.is_some() {
        req.mask_pattern.as_deref()
    } else {
        existing.mask_pattern.as_deref()
    };

    state
        .store
        .update_acl_policy(
            id,
            &name,
            &action,
            properties,
            mask_pattern,
            priority,
            is_active,
        )
        .await
        .map_err(AppError::from)?;

    // Return the updated policy
    let updated = state
        .store
        .get_acl_policy(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("ACL policy"))?;

    Ok(Json(updated))
}

// ---------------------------------------------------------------------------
// DELETE /api/acl/policies/:id — delete policy
// ---------------------------------------------------------------------------

pub(crate) async fn delete_policy(
    State(state): State<AppState>,
    ws: WorkspaceContext,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    ws.require_admin()?;

    let deleted = state
        .store
        .delete_acl_policy(id)
        .await
        .map_err(AppError::from)?;

    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::not_found("ACL policy"))
    }
}

// ---------------------------------------------------------------------------
// GET /api/acl/effective — effective policies for current user
// ---------------------------------------------------------------------------

pub(crate) async fn effective_policies(
    State(state): State<AppState>,
    principal: Principal,
    ws: WorkspaceContext,
) -> Result<Json<Vec<AclPolicy>>, AppError> {
    let user_id = principal.user_uuid().ok();

    let policies = state
        .store
        .get_effective_policies(principal.role.as_str(), ws.workspace_role.as_str(), user_id)
        .await
        .map_err(AppError::from)?;

    Ok(Json(policies))
}
