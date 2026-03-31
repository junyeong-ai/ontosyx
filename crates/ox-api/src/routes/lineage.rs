use axum::Json;
use axum::extract::{Path, State};
use uuid::Uuid;

use ox_store::{LineageEntry, LineageSummary};

use crate::error::AppError;
use crate::state::AppState;

// ---------------------------------------------------------------------------
// GET /api/lineage — summary of lineage per graph label
// ---------------------------------------------------------------------------

pub(crate) async fn get_lineage_summary(
    State(state): State<AppState>,
) -> Result<Json<Vec<LineageSummary>>, AppError> {
    let summary = state
        .store
        .lineage_summary()
        .await
        .map_err(AppError::from)?;
    Ok(Json(summary))
}

// ---------------------------------------------------------------------------
// GET /api/lineage/label/:label — lineage entries for a specific graph label
// ---------------------------------------------------------------------------

pub(crate) async fn get_lineage_for_label(
    State(state): State<AppState>,
    Path(label): Path<String>,
) -> Result<Json<Vec<LineageEntry>>, AppError> {
    let entries = state
        .store
        .get_lineage_for_label(&label)
        .await
        .map_err(AppError::from)?;
    Ok(Json(entries))
}

// ---------------------------------------------------------------------------
// GET /api/lineage/project/:id — lineage entries for a project
// ---------------------------------------------------------------------------

pub(crate) async fn get_lineage_for_project(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<LineageEntry>>, AppError> {
    let entries = state
        .store
        .get_lineage_for_project(id)
        .await
        .map_err(AppError::from)?;
    Ok(Json(entries))
}
