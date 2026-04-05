use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use tracing::warn;
use uuid::Uuid;

use ox_core::InsightSuggestion;
use ox_core::ontology_command::OntologyCommand;
use ox_core::ontology_input::{OntologyInputIR, normalize, to_exchange_format};
use ox_core::ontology_ir::OntologyIR;
use ox_core::source_mapping::SourceMapping;
use ox_store::DesignProject;
use ox_store::ElementVerification;
use ox_store::SavedOntology;
use ox_store::store::CursorParams;

use crate::error::AppError;
use crate::principal::Principal;
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
) -> Result<Json<ox_store::store::CursorPage<SavedOntology>>, AppError> {
    let page = state
        .store
        .list_saved_ontologies(&pagination)
        .await
        .map_err(AppError::from)?;
    Ok(Json(page))
}

// ---------------------------------------------------------------------------
// Normalize — OntologyInputIR → OntologyIR
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/api/ontology/normalize",
    request_body(content = Object, description = "OntologyInputIR to normalize"),
    responses(
        (status = 200, description = "Normalized OntologyIR", body = Object),
        (status = 400, description = "Validation errors", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Ontologies",
)]
pub async fn normalize_ontology(
    Json(input): Json<OntologyInputIR>,
) -> Result<Json<serde_json::Value>, AppError> {
    let result = normalize(input).map_err(|errors| AppError::bad_request(errors.join("; ")))?;
    Ok(Json(serde_json::json!({
        "ontology": result.ontology,
        "warnings": result.warnings,
    })))
}

// ---------------------------------------------------------------------------
// Export — OntologyIR → OntologyInputIR (exchange format)
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/api/ontology/export",
    request_body(content = Object, description = "OntologyIR to export"),
    responses(
        (status = 200, description = "OntologyInputIR exchange format", body = Object),
    ),
    security(("api_key" = [])),
    tag = "Ontologies",
)]
pub async fn export_ontology(
    Json(ontology): Json<OntologyIR>,
) -> Result<Json<OntologyInputIR>, AppError> {
    let exchange = to_exchange_format(&ontology, &SourceMapping::new());
    Ok(Json(exchange))
}

// ---------------------------------------------------------------------------
// Apply commands — batch of OntologyCommand applied to project ontology
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
) -> Result<(StatusCode, Json<OntologyCommandsResponse>), AppError> {
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

    // Auto-invalidate verifications for changed elements
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

    // Validate the resulting ontology
    let errors = ontology.validate();
    if !errors.is_empty() {
        return Err(AppError::unprocessable(errors.join("; ")));
    }

    // Recompute quality report
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
        Json(OntologyCommandsResponse { project: updated }),
    ))
}

// ---------------------------------------------------------------------------
// Export — OntologyIR → Cypher DDL (plain text)
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/api/ontology/export/cypher",
    request_body(content = Object, description = "OntologyIR"),
    responses(
        (status = 200, description = "Cypher DDL statements", content_type = "text/plain"),
    ),
    security(("api_key" = [])),
    tag = "Ontologies",
)]
pub async fn export_cypher(Json(ontology): Json<OntologyIR>) -> Result<String, AppError> {
    Ok(ox_compiler::export::generate_cypher_ddl(&ontology))
}

// ---------------------------------------------------------------------------
// Export — OntologyIR → Mermaid ER diagram (plain text)
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/api/ontology/export/mermaid",
    request_body(content = Object, description = "OntologyIR"),
    responses(
        (status = 200, description = "Mermaid ER diagram", content_type = "text/plain"),
    ),
    security(("api_key" = [])),
    tag = "Ontologies",
)]
pub async fn export_mermaid(Json(ontology): Json<OntologyIR>) -> Result<String, AppError> {
    Ok(ox_compiler::export::generate_mermaid(&ontology))
}

// ---------------------------------------------------------------------------
// Export — OntologyIR → GraphQL Schema (plain text)
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/api/ontology/export/graphql",
    request_body(content = Object, description = "OntologyIR"),
    responses(
        (status = 200, description = "GraphQL schema", content_type = "text/plain"),
    ),
    security(("api_key" = [])),
    tag = "Ontologies",
)]
pub async fn export_graphql(Json(ontology): Json<OntologyIR>) -> Result<String, AppError> {
    Ok(ox_compiler::export::generate_graphql(&ontology))
}

// ---------------------------------------------------------------------------
// Export — OntologyIR → OWL/Turtle (plain text)
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/api/ontology/export/owl",
    request_body(content = Object, description = "OntologyIR"),
    responses(
        (status = 200, description = "OWL/Turtle ontology", content_type = "text/plain"),
    ),
    security(("api_key" = [])),
    tag = "Ontologies",
)]
pub async fn export_owl(Json(ontology): Json<OntologyIR>) -> Result<String, AppError> {
    Ok(ox_compiler::export::generate_owl_turtle(&ontology))
}

// ---------------------------------------------------------------------------
// Export — OntologyIR → SHACL shapes (Turtle format, plain text)
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/api/ontology/export/shacl",
    request_body(content = Object, description = "OntologyIR"),
    responses(
        (status = 200, description = "SHACL shapes in Turtle format", content_type = "text/plain"),
    ),
    security(("api_key" = [])),
    tag = "Ontologies",
)]
pub async fn export_shacl(Json(ontology): Json<OntologyIR>) -> Result<String, AppError> {
    Ok(ox_compiler::export::generate_shacl(&ontology))
}

// ---------------------------------------------------------------------------
// Export — OntologyIR → TypeScript type definitions (plain text)
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/api/ontology/export/typescript",
    request_body(content = Object, description = "OntologyIR"),
    responses(
        (status = 200, description = "TypeScript interface definitions", content_type = "text/plain"),
    ),
    security(("api_key" = [])),
    tag = "Ontologies",
)]
pub async fn export_typescript(Json(ontology): Json<OntologyIR>) -> Result<String, AppError> {
    Ok(ox_compiler::export::generate_typescript(&ontology))
}

// ---------------------------------------------------------------------------
// Export — OntologyIR → Python dataclass definitions (plain text)
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/api/ontology/export/python",
    request_body(content = Object, description = "OntologyIR"),
    responses(
        (status = 200, description = "Python dataclass definitions", content_type = "text/plain"),
    ),
    security(("api_key" = [])),
    tag = "Ontologies",
)]
pub async fn export_python(Json(ontology): Json<OntologyIR>) -> Result<String, AppError> {
    Ok(ox_compiler::export::generate_python(&ontology))
}

// ---------------------------------------------------------------------------
// Import — OWL/Turtle → OntologyIR
// ---------------------------------------------------------------------------

#[derive(Deserialize, utoipa::ToSchema)]
pub struct OntologyImportRequest {
    /// OWL ontology in Turtle format.
    pub content: String,
}

#[utoipa::path(
    post,
    path = "/api/ontology/import/owl",
    request_body = OntologyImportRequest,
    responses(
        (status = 200, description = "Parsed OntologyIR", body = Object),
        (status = 400, description = "Parse or validation errors", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Ontologies",
)]
pub async fn import_owl(
    Json(req): Json<OntologyImportRequest>,
) -> Result<Json<OntologyIR>, AppError> {
    if req.content.trim().is_empty() {
        return Err(AppError::bad_request("content must not be empty"));
    }
    let ontology = ox_compiler::import::parse_owl_turtle(&req.content)
        .map_err(|e| AppError::bad_request(e.to_string()))?;
    Ok(Json(ontology))
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
) -> Result<Json<Vec<InsightSuggestion>>, AppError> {
    let suggestions = state
        .brain
        .suggest_insights(&ontology, None)
        .await
        .map_err(AppError::from)?;
    Ok(Json(suggestions))
}

// ---------------------------------------------------------------------------
// Verification endpoints
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct VerifyElementRequest {
    pub element_id: String,
    pub element_kind: String,
    pub review_notes: Option<String>,
}

/// POST /api/ontology/{id}/verifications — mark an element as verified
pub(crate) async fn verify_element(
    State(state): State<AppState>,
    principal: Principal,
    Path(ontology_id): Path<String>,
    Json(req): Json<VerifyElementRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    if !matches!(req.element_kind.as_str(), "node" | "edge" | "property") {
        return Err(AppError::bad_request(
            "element_kind must be 'node', 'edge', or 'property'",
        ));
    }

    let user = state
        .store
        .get_user_by_provider("ontosyx", &principal.id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("User"))?;

    let verification = ElementVerification {
        id: Uuid::new_v4(),
        ontology_id: ontology_id.clone(),
        element_id: req.element_id,
        element_kind: req.element_kind,
        verified_by: user.id,
        verified_by_name: None,
        review_notes: req.review_notes,
        invalidated_at: None,
        invalidation_reason: None,
        created_at: chrono::Utc::now(),
    };

    let id = state
        .store
        .verify_element(&verification)
        .await
        .map_err(AppError::from)?;

    Ok(Json(serde_json::json!({ "id": id })))
}

/// GET /api/ontology/{id}/verifications — list active verifications
pub(crate) async fn list_verifications(
    State(state): State<AppState>,
    _principal: Principal,
    Path(ontology_id): Path<String>,
) -> Result<Json<Vec<ElementVerification>>, AppError> {
    let verifications = state
        .store
        .get_verifications(&ontology_id)
        .await
        .map_err(AppError::from)?;
    Ok(Json(verifications))
}

/// DELETE /api/ontology/{id}/verifications/{element_id} — revoke verification
pub(crate) async fn delete_verification(
    State(state): State<AppState>,
    principal: Principal,
    Path((ontology_id, element_id)): Path<(String, String)>,
) -> Result<StatusCode, AppError> {
    let user = state
        .store
        .get_user_by_provider("ontosyx", &principal.id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("User"))?;

    state
        .store
        .delete_verification(&ontology_id, &element_id, user.id)
        .await
        .map_err(AppError::from)?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// POST /api/ontology/{id}/reindex — re-index schema embeddings
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/api/ontology/{id}/reindex",
    params(("id" = Uuid, Path, description = "Saved ontology ID")),
    responses(
        (status = 200, description = "Re-indexing triggered", body = inline(ReindexResponse)),
        (status = 404, description = "Ontology not found"),
    ),
    security(("api_key" = [])),
    tag = "Ontologies",
)]
pub async fn reindex_schema(
    State(state): State<AppState>,
    _principal: Principal,
    Path(ontology_id): Path<Uuid>,
) -> Result<Json<ReindexResponse>, AppError> {
    let saved: SavedOntology = state
        .store
        .get_saved_ontology(ontology_id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Ontology not found"))?;

    let ontology: OntologyIR = serde_json::from_value(saved.ontology_ir)
        .map_err(|e| AppError::internal(format!("Failed to deserialize ontology: {e}")))?;

    let node_count = ontology.node_types.len();
    let memory = state
        .memory
        .as_ref()
        .ok_or_else(|| AppError::bad_request("Semantic memory not configured"))?;

    ox_brain::schema_rag::index_ontology_schema(memory, &ontology, &ontology_id.to_string()).await;

    Ok(Json(ReindexResponse {
        ontology_id,
        nodes_indexed: node_count,
    }))
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct ReindexResponse {
    pub ontology_id: Uuid,
    pub nodes_indexed: usize,
}

// ---------------------------------------------------------------------------
// POST /api/ontology/{id}/audit — compare ontology against live graph
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/api/ontology/{id}/audit",
    params(("id" = Uuid, Path, description = "Saved ontology ID")),
    responses(
        (status = 200, description = "Audit report comparing ontology vs graph", body = Object),
        (status = 404, description = "Ontology not found"),
        (status = 503, description = "Graph database not connected"),
    ),
    security(("api_key" = [])),
    tag = "Ontologies",
)]
pub async fn audit_graph(
    State(state): State<AppState>,
    _principal: Principal,
    Path(ontology_id): Path<Uuid>,
) -> Result<Json<ox_core::graph_audit::GraphAuditReport>, AppError> {
    let saved: SavedOntology = state
        .store
        .get_saved_ontology(ontology_id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Ontology not found"))?;

    let ontology: OntologyIR = serde_json::from_value(saved.ontology_ir)
        .map_err(|e| AppError::internal(format!("Failed to deserialize ontology: {e}")))?;

    let runtime = state
        .runtime
        .as_ref()
        .ok_or_else(|| AppError::service_unavailable("Graph database not connected"))?;

    let overview =
        tokio::time::timeout(std::time::Duration::from_secs(10), runtime.graph_overview())
            .await
            .map_err(|_| AppError::internal("Graph overview timed out"))?
            .map_err(AppError::from)?;

    let report = ox_core::graph_audit::audit_graph(&ontology, &overview);
    Ok(Json(report))
}

// ---------------------------------------------------------------------------
// POST /api/ontology/adopt-graph — create ontology from live graph labels
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/api/ontology/adopt-graph",
    request_body(content = AdoptGraphRequest, description = "Name for the adopted ontology"),
    responses(
        (status = 200, description = "Ontology created from graph schema", body = Object),
        (status = 503, description = "Graph database not connected"),
    ),
    security(("api_key" = [])),
    tag = "Ontologies",
)]
pub async fn adopt_graph(
    State(state): State<AppState>,
    _principal: Principal,
    Json(req): Json<AdoptGraphRequest>,
) -> Result<Json<OntologyIR>, AppError> {
    let runtime = state
        .runtime
        .as_ref()
        .ok_or_else(|| AppError::service_unavailable("Graph database not connected"))?;

    let overview =
        tokio::time::timeout(std::time::Duration::from_secs(10), runtime.graph_overview())
            .await
            .map_err(|_| AppError::internal("Graph overview timed out"))?
            .map_err(AppError::from)?;

    let name = req
        .name
        .unwrap_or_else(|| "Adopted Graph Ontology".to_string());
    let ontology = ox_core::graph_audit::ontology_from_graph(&overview, &name);

    // Save to database if requested (makes it usable in Analyze mode)
    if req.save.unwrap_or(false) {
        let ontology_ir = serde_json::to_value(&ontology)
            .map_err(|e| AppError::internal(format!("Failed to serialize ontology: {e}")))?;

        let saved_id = state
            .store
            .create_standalone_ontology(&name, &ontology_ir)
            .await
            .map_err(AppError::from)?;

        // Re-index schema embeddings for the saved ontology
        if let Some(memory) = &state.memory {
            let memory = std::sync::Arc::clone(memory);
            let ont = ontology.clone();
            tokio::spawn(async move {
                ox_brain::schema_rag::index_ontology_schema(&memory, &ont, &saved_id.to_string())
                    .await;
            });
        }

        tracing::info!(
            saved_ontology_id = %saved_id,
            nodes = ontology.node_types.len(),
            edges = ontology.edge_types.len(),
            "Graph ontology adopted and saved"
        );
    }

    Ok(Json(ontology))
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct AdoptGraphRequest {
    pub name: Option<String>,
    /// If true, persist the adopted ontology to database for use in Analyze mode.
    #[serde(default)]
    pub save: Option<bool>,
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
) -> Result<Json<EnrichResponse>, AppError> {
    let runtime = state.runtime.as_ref().ok_or_else(AppError::no_runtime)?;

    let saved = state
        .store
        .get_saved_ontology(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Saved ontology"))?;

    let ontology: OntologyIR = serde_json::from_value(saved.ontology_ir.clone())
        .map_err(|e| AppError::internal(format!("Failed to parse ontology IR: {e}")))?;

    // Profile graph data
    let config = ox_runtime::profiler::ProfileConfig::for_ontology_size(ontology.node_types.len());
    let profile = ox_runtime::profiler::profile_graph(runtime.as_ref(), &ontology, &config)
        .await
        .map_err(AppError::from)?;

    let profiled_nodes = profile.node_profiles.len();
    let profiled_edges = profile.edge_profiles.len();

    // Enrich descriptions
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

    // Apply if requested
    if req.apply && !result.changes.is_empty() {
        let ir_json = serde_json::to_value(&result.ontology).map_err(|e| {
            AppError::internal(format!("Failed to serialize enriched ontology: {e}"))
        })?;
        state
            .store
            .update_ontology_ir(id, &ir_json)
            .await
            .map_err(AppError::from)?;

        // Re-index schema embeddings (fire-and-forget)
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

    Ok(Json(EnrichResponse {
        ontology_id: id,
        changes,
        profiled_nodes,
        profiled_edges,
        applied: req.apply,
    }))
}
