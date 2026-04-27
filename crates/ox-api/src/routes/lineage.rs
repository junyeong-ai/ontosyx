use axum::Json;
use axum::extract::{Path, State};
use uuid::Uuid;

use ox_store::{LineageEntry, LineageSummary};

use crate::error::AppError;
use crate::principal::Principal;
use crate::response::ApiResponse;
use crate::state::AppState;

// ---------------------------------------------------------------------------
// Lineage — read-only introspection over ontology → source bindings.
//
// Gated to `designer` rather than `viewer`. Lineage exposes the
// exact column and foreign-key relationships that the graph
// schema is built on top of, which is load-bearing information for
// ontology editors but not something a read-only analytics viewer
// needs to see — a viewer's job is to query the graph, not to
// understand the physical substrate.
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// GET /api/lineage — summary of lineage per graph label
// ---------------------------------------------------------------------------

pub(crate) async fn get_lineage_summary(
    State(state): State<AppState>,
    principal: Principal,
) -> Result<Json<ApiResponse<Vec<LineageSummary>>>, AppError> {
    principal.require_designer()?;
    let summary = state
        .store
        .lineage_summary()
        .await
        .map_err(AppError::from)?;
    Ok(ApiResponse::of(summary))
}

// ---------------------------------------------------------------------------
// GET /api/lineage/label/:label — lineage entries for a specific graph label
// ---------------------------------------------------------------------------

pub(crate) async fn list_lineage_for_label(
    State(state): State<AppState>,
    principal: Principal,
    Path(label): Path<String>,
) -> Result<Json<ApiResponse<Vec<LineageEntry>>>, AppError> {
    principal.require_designer()?;
    let entries = state
        .store
        .list_lineage_for_label(&label)
        .await
        .map_err(AppError::from)?;
    Ok(ApiResponse::of(entries))
}

// ---------------------------------------------------------------------------
// GET /api/lineage/project/:id — lineage entries for a project
// ---------------------------------------------------------------------------

pub(crate) async fn get_lineage_for_project(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<Vec<LineageEntry>>>, AppError> {
    principal.require_designer()?;
    let entries = state
        .store
        .list_lineage_for_project(id)
        .await
        .map_err(AppError::from)?;
    Ok(ApiResponse::of(entries))
}
