use axum::Json;
use axum::extract::{Path, State};
use uuid::Uuid;

use ox_core::source_schema::SourceSchema;

use crate::error::AppError;
use crate::principal::Principal;
use crate::response::ApiResponse;
use crate::state::AppState;

use super::helpers::validate_decisions;
use super::types::{OntologyDraftView, UpdateOntologyDraftDecisionsRequest};

// ---------------------------------------------------------------------------
// PATCH /api/ontology-drafts/:id/decisions
// ---------------------------------------------------------------------------

#[utoipa::path(
    patch,
    path = "/api/ontology-drafts/{id}/decisions",
    params(("id" = Uuid, Path, description = "Ontology draft ID")),
    request_body = UpdateOntologyDraftDecisionsRequest,
    responses(
        (status = 200, description = "Decisions updated", body = Object),
        (status = 404, description = "Ontology draft not found", body = inline(crate::openapi::ErrorResponse)),
        (status = 409, description = "Revision conflict", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Ontology Drafts",
)]
pub(crate) async fn update_decisions(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateOntologyDraftDecisionsRequest>,
) -> Result<Json<ApiResponse<OntologyDraftView>>, AppError> {
    principal.require_designer()?;
    let project = state
        .store
        .get_ontology_draft(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(AppError::ontology_draft_not_found)?;

    // Validate decisions against stored schema (if available)
    if let Some(schema_val) = &project.source_schema {
        let schema: SourceSchema = serde_json::from_value(schema_val.clone())
            .map_err(|e| AppError::internal(format!("Corrupt source_schema: {e}")))?;
        validate_decisions(&req.design_options, &schema)?;
    }

    let options_json = serde_json::to_value(&req.design_options)
        .map_err(|e| AppError::internal(format!("serialize design_options: {e}")))?;

    state
        .store
        .update_design_options(id, &options_json, req.revision)
        .await
        .map_err(AppError::from)?;

    let project = state
        .store
        .get_ontology_draft(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(AppError::ontology_draft_not_found)?;

    Ok(ApiResponse::of(OntologyDraftView::from_ontology_draft(project)))
}
