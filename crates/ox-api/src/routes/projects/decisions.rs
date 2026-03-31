use axum::Json;
use axum::extract::{Path, State};
use uuid::Uuid;

use ox_core::source_schema::SourceSchema;

use crate::error::AppError;
use crate::principal::Principal;
use crate::state::AppState;

use super::helpers::validate_decisions;
use super::types::UpdateDecisionsRequest;
use ox_store::DesignProject;

// ---------------------------------------------------------------------------
// PATCH /api/projects/:id/decisions
// ---------------------------------------------------------------------------

#[utoipa::path(
    patch,
    path = "/api/projects/{id}/decisions",
    params(("id" = Uuid, Path, description = "Project ID")),
    request_body = UpdateDecisionsRequest,
    responses(
        (status = 200, description = "Decisions updated", body = Object),
        (status = 404, description = "Project not found", body = inline(crate::openapi::ErrorResponse)),
        (status = 409, description = "Revision conflict", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Projects",
)]
pub(crate) async fn update_decisions(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateDecisionsRequest>,
) -> Result<Json<DesignProject>, AppError> {
    principal.require_designer()?;
    let project = state
        .store
        .get_design_project(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(AppError::project_not_found)?;

    // Validate decisions against stored schema (if available)
    if let Some(schema_val) = &project.source_schema {
        let schema: SourceSchema = serde_json::from_value(schema_val.clone())
            .map_err(|e| AppError::internal(format!("Corrupt source_schema: {e}")))?;
        validate_decisions(&req.design_options, &schema)?;
    }

    let options_json = serde_json::to_value(&req.design_options)
        .map_err(|e| AppError::bad_request(format!("Invalid design_options: {e}")))?;

    state
        .store
        .update_design_options(id, &options_json, req.revision)
        .await
        .map_err(AppError::from)?;

    let project = state
        .store
        .get_design_project(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(AppError::project_not_found)?;

    Ok(Json(project))
}
