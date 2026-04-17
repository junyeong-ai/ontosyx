use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use tracing::warn;
use uuid::Uuid;

use ox_core::InsightSuggestion;
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
pub async fn list_ontologies(
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
pub async fn apply_ontology_commands(
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

// ---------------------------------------------------------------------------
// POST /api/ontology/suggestions — proactive insight suggestions
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/api/ontology/suggestions",
    request_body(content = Object, description = "OntologyIR to generate suggestions for"),
    responses(
        (status = 200, description = "List of insight suggestions", body = Vec<Object>),
    ),
    security(("api_key" = [])),
    tag = "Ontologies",
)]
pub async fn suggest_insights(
    State(state): State<AppState>,
    _principal: Principal,
    Json(ontology): Json<OntologyIR>,
) -> Result<Json<ApiResponse<Vec<InsightSuggestion>>>, AppError> {
    let suggestions = state
        .brain
        .suggest_insights(&ontology, None)
        .await
        .map_err(AppError::from)?;
    Ok(ApiResponse::of(suggestions))
}

// ---------------------------------------------------------------------------
// POST /api/ontologies/:id/enrich — enrich descriptions with data samples
// ---------------------------------------------------------------------------

#[derive(Serialize, utoipa::ToSchema)]
pub struct EnrichResponse {
    pub ontology_id: Uuid,
    pub changes: Vec<EnrichChange>,
    pub profiled_nodes: usize,
    pub profiled_edges: usize,
    pub applied: bool,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct EnrichChange {
    pub entity_label: String,
    pub entity_kind: String,
    pub property_name: String,
    pub old_description: Option<String>,
    pub new_description: String,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct EnrichRequest {
    /// If true, save the enriched ontology. If false, preview only (dry run).
    #[serde(default)]
    pub apply: bool,
}

#[utoipa::path(
    post,
    path = "/api/ontologies/{id}/enrich",
    request_body = EnrichRequest,
    responses(
        (status = 200, description = "Enrichment result", body = EnrichResponse),
    ),
    tag = "Ontology",
)]
pub async fn enrich_ontology(
    State(state): State<AppState>,
    _principal: Principal,
    Path(id): Path<Uuid>,
    Json(req): Json<EnrichRequest>,
) -> Result<Json<ApiResponse<EnrichResponse>>, AppError> {
    let runtime = state.runtime.as_ref().ok_or_else(AppError::no_runtime)?;

    let saved = state
        .store
        .get_saved_ontology(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Saved ontology"))?;

    let ontology: OntologyIR = serde_json::from_value(saved.ontology_ir.clone())
        .map_err(|e| AppError::internal(format!("Failed to parse ontology IR: {e}")))?;

    let config = ox_runtime::profiler::ProfileConfig::for_ontology_size(ontology.node_types.len());
    let profile = ox_runtime::profiler::profile_graph(runtime.as_ref(), &ontology, &config)
        .await
        .map_err(AppError::from)?;

    let profiled_nodes = profile.node_profiles.len();
    let profiled_edges = profile.edge_profiles.len();

    let result = ox_runtime::enrichment::enrich_descriptions(&ontology, &profile);

    let changes: Vec<EnrichChange> = result
        .changes
        .iter()
        .map(|c| EnrichChange {
            entity_label: c.entity_label.clone(),
            entity_kind: c.entity_kind.to_string(),
            property_name: c.property_name.clone(),
            old_description: c.old_description.clone(),
            new_description: c.new_description.clone(),
        })
        .collect();

    if req.apply && !result.changes.is_empty() {
        let ir_json = serde_json::to_value(&result.ontology).map_err(|e| {
            AppError::internal(format!("Failed to serialize enriched ontology: {e}"))
        })?;
        state
            .store
            .update_ontology_ir(id, &ir_json)
            .await
            .map_err(AppError::from)?;

        if let Some(memory) = &state.memory {
            let memory = std::sync::Arc::clone(memory);
            let ont_id = id.to_string();
            let enriched = result.ontology.clone();
            crate::spawn_scoped::spawn_scoped(async move {
                ox_brain::schema_rag::index_ontology_schema(&memory, &enriched, &ont_id).await;
            });
        }

        tracing::info!(
            ontology_id = %id,
            changes = changes.len(),
            "Ontology descriptions enriched with data samples"
        );
    }

    Ok(ApiResponse::of(EnrichResponse {
        ontology_id: id,
        changes,
        profiled_nodes,
        profiled_edges,
        applied: req.apply,
    }))
}
