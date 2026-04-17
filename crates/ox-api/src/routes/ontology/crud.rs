use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use tracing::warn;
use uuid::Uuid;

use ox_core::ontology_command::OntologyCommand;
use ox_core::ontology_ir::OntologyIR;
use ox_store::DesignProject;
use ox_store::SavedOntology;
use ox_store::store::CursorParams;

use crate::error::AppError;
use crate::principal::Principal;
use crate::response::ApiResponse;
use crate::routes::projects::helpers::{
    assess_quality_from_project, get_design_options, load_mutable_project, reload_project,
};
use crate::state::AppState;

// ---------------------------------------------------------------------------
// GET /api/ontologies — list saved ontologies
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/ontologies",
    params(
        ("limit" = Option<u32>, Query, description = "Max items to return (default 50, max 100)"),
        ("cursor" = Option<String>, Query, description = "Opaque cursor from a previous response"),
    ),
    responses(
        (status = 200, description = "Paginated list of saved ontologies", body = Object),
    ),
    security(("api_key" = [])),
    tag = "Ontologies",
)]
pub(crate) async fn list_ontologies(
    State(state): State<AppState>,
    axum::extract::Query(pagination): axum::extract::Query<CursorParams>,
) -> Result<Json<ApiResponse<Vec<SavedOntology>>>, AppError> {
    let page = state
        .store
        .list_saved_ontologies(&pagination)
        .await
        .map_err(AppError::from)?;
    Ok(ApiResponse::page(page))
}

// ---------------------------------------------------------------------------
// PATCH /api/projects/{id}/ontology — apply batch of OntologyCommand
// ---------------------------------------------------------------------------

#[derive(Deserialize, utoipa::ToSchema)]
pub struct OntologyCommandsRequest {
    pub revision: i32,
    /// List of ontology mutation commands.
    #[schema(value_type = Vec<Object>)]
    pub commands: Vec<OntologyCommand>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct OntologyCommandsResponse {
    #[schema(value_type = Object)]
    pub project: DesignProject,
}

#[utoipa::path(
    patch,
    path = "/api/projects/{id}/ontology",
    params(
        ("id" = Uuid, Path, description = "Design project ID"),
    ),
    request_body = OntologyCommandsRequest,
    responses(
        (status = 200, description = "Commands applied", body = OntologyCommandsResponse),
        (status = 400, description = "Empty commands or invalid ontology", body = inline(crate::openapi::ErrorResponse)),
        (status = 404, description = "Project not found", body = inline(crate::openapi::ErrorResponse)),
        (status = 422, description = "Command execution or validation failed", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Ontologies",
)]
pub(crate) async fn apply_ontology_commands(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
    Json(req): Json<OntologyCommandsRequest>,
) -> Result<(StatusCode, Json<ApiResponse<OntologyCommandsResponse>>), AppError> {
    principal.require_designer()?;
    if req.commands.is_empty() {
        return Err(AppError::bad_request("commands must not be empty"));
    }

    let project = load_mutable_project(&state, id).await?;

    // Snapshot current state before mutation (best-effort)
    if let Some(ont) = &project.ontology
        && let Err(e) = state
            .store
            .create_ontology_snapshot(
                id,
                project.revision,
                ont,
                project.source_mapping.as_ref(),
                project.quality_report.as_ref(),
            )
            .await
    {
        warn!(project_id = %id, error = %e, "Failed to save ontology snapshot");
    }

    let mut ontology: OntologyIR = match project.ontology.as_ref() {
        None => return Err(AppError::no_ontology()),
        Some(v) => serde_json::from_value(v.clone())
            .map_err(|e| AppError::internal(format!("Corrupt ontology in project: {e}")))?,
    };

    // Apply each command sequentially, tracking changed element IDs
    let mut changed_element_ids: Vec<String> = Vec::new();
    for cmd in &req.commands {
        changed_element_ids.extend(cmd.affected_element_ids());
        let result = cmd.execute(&ontology).map_err(AppError::unprocessable)?;
        ontology = result.new_ontology;
    }

    if !changed_element_ids.is_empty() {
        let id_refs: Vec<&str> = changed_element_ids.iter().map(|s| s.as_str()).collect();
        if let Err(e) = state
            .store
            .invalidate_for_elements(&ontology.id, &id_refs, "ontology_command")
            .await
        {
            warn!(error = %e, "Failed to invalidate verifications for changed elements");
        }
    }

    let errors = ontology.validate();
    if !errors.is_empty() {
        return Err(AppError::unprocessable(errors.join("; ")));
    }

    let opts = get_design_options(&project);
    let quality_report = assess_quality_from_project(
        &project,
        &ontology,
        &opts.excluded_tables,
        &opts.column_clarifications,
    )?;

    let ontology_json = AppError::to_json(&ontology)?;
    let qr_json = AppError::to_json(&quality_report)?;

    state
        .store
        .update_design_result(
            id,
            &ontology_json,
            project.source_mapping.as_ref(),
            Some(&qr_json),
            req.revision,
        )
        .await
        .map_err(AppError::from)?;

    let updated = reload_project(&state, id).await?;

    Ok((
        StatusCode::OK,
        ApiResponse::of(OntologyCommandsResponse { project: updated }),
    ))
}
