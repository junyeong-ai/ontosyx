use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use chrono::Utc;
use serde::Deserialize;
use uuid::Uuid;

use ox_store::{CursorPage, CursorParams, PinboardItem};

use crate::error::AppError;
use crate::principal::Principal;
use crate::state::AppState;

// ---------------------------------------------------------------------------
// POST /api/pins — pin a query execution result
// ---------------------------------------------------------------------------

#[derive(Deserialize, utoipa::ToSchema)]
pub struct PinCreateRequest {
    pub query_execution_id: Uuid,
    /// Widget specification JSON.
    pub widget_spec: serde_json::Value,
    pub title: Option<String>,
}

#[utoipa::path(
    post,
    path = "/api/pins",
    request_body = PinCreateRequest,
    responses(
        (status = 201, description = "Pin created", body = Object),
    ),
    tag = "Pins",
)]
pub async fn create_pin(
    State(state): State<AppState>,
    principal: Principal,
    Json(req): Json<PinCreateRequest>,
) -> Result<(StatusCode, Json<PinboardItem>), AppError> {
    let item = PinboardItem {
        id: Uuid::new_v4(),
        query_execution_id: req.query_execution_id,
        user_id: principal.id.clone(),
        widget_spec: req.widget_spec,
        title: req.title,
        pinned_at: Utc::now(),
    };

    state.store.create_pin(&principal.id, &item).await?;
    Ok((StatusCode::CREATED, Json(item)))
}

// ---------------------------------------------------------------------------
// GET /api/pins?limit=50&cursor=... — list pins (cursor-paginated)
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/pins",
    params(
        ("limit" = Option<u32>, Query, description = "Max items to return (default 50, max 100)"),
        ("cursor" = Option<String>, Query, description = "Opaque cursor from a previous response"),
    ),
    responses(
        (status = 200, description = "Paginated pin list", body = Object),
    ),
    tag = "Pins",
)]
pub async fn list_pins(
    State(state): State<AppState>,
    principal: Principal,
    Query(params): Query<CursorParams>,
) -> Result<Json<CursorPage<PinboardItem>>, AppError> {
    let page = state.store.list_pins(&principal.id, &params).await?;
    Ok(Json(page))
}

// ---------------------------------------------------------------------------
// DELETE /api/pins/:id — unpin an item
// ---------------------------------------------------------------------------

#[utoipa::path(
    delete,
    path = "/api/pins/{id}",
    params(
        ("id" = Uuid, Path, description = "Pin ID"),
    ),
    responses(
        (status = 204, description = "Pin deleted"),
        (status = 404, description = "Pin not found", body = inline(crate::openapi::ErrorResponse)),
    ),
    tag = "Pins",
)]
pub async fn delete_pin(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let deleted = state.store.delete_pin(&principal.id, id).await?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::pin_not_found())
    }
}
