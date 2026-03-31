use axum::Json;
use axum::extract::{Path, State};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use ox_store::store::{CursorPage, CursorParams};

use crate::error::AppError;
use crate::principal::{Principal, VALID_PLATFORM_ROLES};
use crate::state::AppState;

use super::auth::UserInfo;

// ---------------------------------------------------------------------------
// GET /api/users — list all users (any authenticated user)
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/users",
    params(
        ("limit" = Option<u32>, Query, description = "Max items (default 50, max 100)"),
        ("cursor" = Option<String>, Query, description = "Pagination cursor"),
    ),
    responses(
        (status = 200, description = "User list"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer" = [])),
    tag = "Users",
)]
pub(crate) async fn list_users(
    State(state): State<AppState>,
    _principal: Principal, // auth required; no role restriction
    axum::extract::Query(pagination): axum::extract::Query<CursorParams>,
) -> Result<Json<CursorPage<UserInfo>>, AppError> {
    let page = state
        .store
        .list_users(&pagination)
        .await
        .map_err(AppError::from)?;

    let items = page
        .items
        .into_iter()
        .map(|u| UserInfo {
            id: u.id,
            email: u.email,
            name: u.name,
            picture: u.picture,
            role: u.role,
        })
        .collect();

    Ok(Json(CursorPage {
        items,
        next_cursor: page.next_cursor,
    }))
}

// ---------------------------------------------------------------------------
// PATCH /api/users/:id/role — update a user's role (admin only)
// ---------------------------------------------------------------------------

#[derive(Deserialize, utoipa::ToSchema)]
pub struct UserRoleUpdateRequest {
    pub role: String,
}

#[derive(Serialize)]
pub(crate) struct UserRoleUpdateResponse {
    pub user: UserInfo,
}

#[utoipa::path(
    patch,
    path = "/api/users/{id}/role",
    params(("id" = Uuid, Path, description = "User ID")),
    request_body = UserRoleUpdateRequest,
    responses(
        (status = 200, description = "Role updated"),
        (status = 400, description = "Invalid role"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "User not found"),
    ),
    security(("bearer" = [])),
    tag = "Users",
)]
pub(crate) async fn update_user_role(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
    Json(req): Json<UserRoleUpdateRequest>,
) -> Result<Json<UserRoleUpdateResponse>, AppError> {
    principal.require_admin()?;

    if principal.id == id.to_string() {
        return Err(AppError::bad_request("Cannot change your own role"));
    }

    if !VALID_PLATFORM_ROLES.contains(&req.role.as_str()) {
        return Err(AppError::bad_request(format!(
            "Invalid role '{}'. Valid roles: {}",
            req.role,
            VALID_PLATFORM_ROLES.join(", "),
        )));
    }

    let old_role = state
        .store
        .get_user_by_id(id)
        .await
        .map_err(AppError::from)?
        .map(|u| u.role)
        .unwrap_or_default();

    state
        .store
        .update_user_role(id, &req.role)
        .await
        .map_err(AppError::from)?;

    let user = state
        .store
        .get_user_by_id(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("User"))?;

    tracing::info!(
        actor = %principal.id,
        target_user = %id,
        old_role = %old_role,
        new_role = %req.role,
        "User role updated"
    );

    Ok(Json(UserRoleUpdateResponse {
        user: UserInfo {
            id: user.id,
            email: user.email,
            name: user.name,
            picture: user.picture,
            role: user.role,
        },
    }))
}
