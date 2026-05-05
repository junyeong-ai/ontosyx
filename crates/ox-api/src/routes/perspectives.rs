use axum::Json;
use axum::extract::{Path, Query, State};
use chrono::Utc;
use serde::Deserialize;
use uuid::Uuid;

use ox_store::WorkbenchPerspective;

use crate::error::AppError;
use crate::principal::Principal;
use crate::response::ApiResponse;
use crate::state::AppState;
use crate::workspace::WorkspaceContext;

// ---------------------------------------------------------------------------
// Request types
// ---------------------------------------------------------------------------

#[derive(Deserialize, utoipa::ToSchema)]
pub struct UpsertPerspectiveRequest {
    pub lineage_id: String,
    pub topology_signature: String,
    pub ontology_draft_id: Option<Uuid>,
    pub name: String,
    /// Node positions JSON.
    pub positions: serde_json::Value,
    /// Viewport state JSON.
    pub viewport: serde_json::Value,
    /// Filter settings JSON.
    #[serde(default)]
    pub filters: serde_json::Value,
    /// Collapsed group settings JSON.
    #[serde(default)]
    pub collapsed_groups: serde_json::Value,
    #[serde(default)]
    pub is_default: bool,
}

// ---------------------------------------------------------------------------
// PUT /api/perspectives — save (upsert)
// ---------------------------------------------------------------------------

#[utoipa::path(
    put,
    path = "/api/perspectives",
    request_body = UpsertPerspectiveRequest,
    responses(
        (status = 200, description = "Perspective saved", body = Object),
    ),
    tag = "Perspectives",
)]
pub(crate) async fn save_perspective(
    State(state): State<AppState>,
    principal: Principal,
    ws: WorkspaceContext,
    Json(req): Json<UpsertPerspectiveRequest>,
) -> Result<Json<ApiResponse<WorkbenchPerspective>>, AppError> {
    let perspective = WorkbenchPerspective {
        id: Uuid::new_v4(),
        user_id: principal.id.clone(),
        workspace_id: ws.workspace_id,
        lineage_id: req.lineage_id.clone(),
        topology_signature: req.topology_signature,
        ontology_draft_id: req.ontology_draft_id,
        name: req.name.clone(),
        positions: req.positions,
        viewport: req.viewport,
        filters: req.filters,
        collapsed_groups: req.collapsed_groups,
        is_default: req.is_default,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    state
        .store
        .upsert_perspective(&perspective)
        .await
        .map_err(AppError::from)?;

    let saved = state
        .store
        .get_perspective(&principal.id, &req.lineage_id, &req.name)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::internal("Failed to retrieve saved perspective"))?;

    Ok(ApiResponse::of(saved))
}

// ---------------------------------------------------------------------------
// GET /api/perspectives/by-lineage/:lineage_id — list for lineage
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/perspectives/by-lineage/{lineage_id}",
    params(
        ("lineage_id" = String, Path, description = "Lineage ID"),
    ),
    responses(
        (status = 200, description = "List of perspectives for this lineage", body = Object),
    ),
    tag = "Perspectives",
)]
pub(crate) async fn list_perspectives(
    State(state): State<AppState>,
    principal: Principal,
    Path(lineage_id): Path<String>,
) -> Result<Json<ApiResponse<Vec<WorkbenchPerspective>>>, AppError> {
    let perspectives = state
        .store
        .list_perspectives(&principal.id, &lineage_id)
        .await
        .map_err(AppError::from)?;

    Ok(ApiResponse::of(perspectives))
}

// ---------------------------------------------------------------------------
// GET /api/perspectives/by-lineage/:lineage_id/default — get default
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/perspectives/by-lineage/{lineage_id}/default",
    params(
        ("lineage_id" = String, Path, description = "Lineage ID"),
    ),
    responses(
        (status = 200, description = "Default perspective (null if none set)", body = Object),
    ),
    tag = "Perspectives",
)]
pub(crate) async fn find_default_perspective(
    State(state): State<AppState>,
    principal: Principal,
    Path(lineage_id): Path<String>,
) -> Result<Json<ApiResponse<Option<WorkbenchPerspective>>>, AppError> {
    let perspective = state
        .store
        .find_default_perspective(&principal.id, &lineage_id)
        .await
        .map_err(AppError::from)?;

    Ok(ApiResponse::of(perspective))
}

// ---------------------------------------------------------------------------
// GET /api/perspectives/by-lineage/:lineage_id/best?topology_signature=...
// ---------------------------------------------------------------------------

/// 2-tier perspective lookup: exact lineage match, then topology match.
#[utoipa::path(
    get,
    path = "/api/perspectives/by-lineage/{lineage_id}/best",
    params(
        ("lineage_id" = String, Path, description = "Lineage ID"),
        ("topology_signature" = String, Query, description = "Topology hash for fallback matching"),
    ),
    responses(
        (status = 200, description = "Best matching perspective (null if none)", body = Object),
    ),
    tag = "Perspectives",
)]
pub(crate) async fn find_best_perspective(
    State(state): State<AppState>,
    principal: Principal,
    Path(lineage_id): Path<String>,
    Query(params): Query<PerspectiveFindParams>,
) -> Result<Json<ApiResponse<Option<WorkbenchPerspective>>>, AppError> {
    let perspective = state
        .store
        .find_best_perspective(&principal.id, &lineage_id, &params.topology_signature)
        .await
        .map_err(AppError::from)?;
    Ok(ApiResponse::of(perspective))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct PerspectiveFindParams {
    pub topology_signature: String,
}

// ---------------------------------------------------------------------------
// DELETE /api/perspectives/:id — delete
// ---------------------------------------------------------------------------

#[utoipa::path(
    delete,
    path = "/api/perspectives/{id}",
    params(
        ("id" = Uuid, Path, description = "Perspective ID"),
    ),
    responses(
        (status = 200, description = "Perspective deleted", body = Object),
        (status = 404, description = "Perspective not found", body = inline(crate::openapi::ErrorResponse)),
    ),
    tag = "Perspectives",
)]
pub(crate) async fn delete_perspective(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    let deleted = state
        .store
        .delete_perspective(&principal.id, id)
        .await
        .map_err(AppError::from)?;

    if !deleted {
        return Err(AppError::perspective_not_found());
    }

    Ok(ApiResponse::of(serde_json::json!({ "deleted": true })))
}
