use axum::Json;
use axum::extract::{Path, State};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use ox_core::ontology_ir::OntologyIR;
use ox_store::SavedOntology;

use crate::error::AppError;
use crate::principal::Principal;
use crate::response::ApiResponse;
use crate::state::AppState;

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
pub(crate) async fn reindex_schema(
    State(state): State<AppState>,
    _principal: Principal,
    Path(ontology_id): Path<Uuid>,
) -> Result<Json<ApiResponse<ReindexResponse>>, AppError> {
    let saved: SavedOntology = state
        .store
        .get_saved_ontology(ontology_id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Ontology not found"))?;

    let ontology: OntologyIR = serde_json::from_value(saved.ontology_ir)
        .map_err(|e| AppError::internal(format!("Failed to deserialize ontology: {e}")))?;

    let node_count = ontology.node_types().len();
    let memory = state
        .memory
        .as_ref()
        .ok_or_else(|| AppError::bad_request("Semantic memory not configured"))?;

    ox_brain::schema_rag::index_ontology_schema(memory, &ontology, &ontology_id.to_string()).await;

    Ok(ApiResponse::of(ReindexResponse {
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
pub(crate) async fn graph_audit_report(
    State(state): State<AppState>,
    _principal: Principal,
    Path(ontology_id): Path<Uuid>,
) -> Result<Json<ApiResponse<ox_core::graph_audit::GraphAuditReport>>, AppError> {
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
    Ok(ApiResponse::of(report))
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
pub(crate) async fn adopt_graph(
    State(state): State<AppState>,
    _principal: Principal,
    Json(req): Json<AdoptGraphRequest>,
) -> Result<Json<ApiResponse<OntologyIR>>, AppError> {
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

    if req.save.unwrap_or(false) {
        let ontology_ir = serde_json::to_value(&ontology)
            .map_err(|e| AppError::internal(format!("Failed to serialize ontology: {e}")))?;

        let saved_id = state
            .store
            .create_standalone_ontology(&name, &ontology_ir)
            .await
            .map_err(AppError::from)?;

        // Re-index schema embeddings for the saved ontology.
        // Must use spawn_with_ws so the spawned task inherits the
        // workspace-scoped task-locals for pgvector RLS.
        if let Some(memory) = &state.memory {
            let memory = std::sync::Arc::clone(memory);
            let ont = ontology.clone();
            let ws_scope = crate::spawn_scoped::WsScope::capture();
            crate::spawn_scoped::spawn_with_ws(ws_scope, async move {
                ox_brain::schema_rag::index_ontology_schema(&memory, &ont, &saved_id.to_string())
                    .await;
            });
        }

        tracing::info!(
            saved_ontology_id = %saved_id,
            nodes = ontology.node_types().len(),
            edges = ontology.edge_types().len(),
            "Graph ontology adopted and saved"
        );
    }

    Ok(ApiResponse::of(ontology))
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct AdoptGraphRequest {
    pub name: Option<String>,
    /// If true, persist the adopted ontology to database for use in Analyze mode.
    #[serde(default)]
    pub save: Option<bool>,
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
pub(crate) async fn suggest_insights(
    State(state): State<AppState>,
    _principal: Principal,
    Json(ontology): Json<OntologyIR>,
) -> Result<Json<ApiResponse<Vec<ox_core::InsightSuggestion>>>, AppError> {
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

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct EnrichResponse {
    pub ontology_id: Uuid,
    pub changes: Vec<EnrichChange>,
    pub profiled_nodes: usize,
    pub profiled_edges: usize,
    pub applied: bool,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct EnrichChange {
    pub entity_label: String,
    pub entity_kind: String,
    pub property_name: String,
    /// Previous description resolved to plain text (pre-enrichment). `None`
    /// when the property had no prior description.
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
pub(crate) async fn enrich_ontology(
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

    let config = ox_runtime::profiler::ProfileConfig::for_ontology_size(ontology.node_types().len());
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
            old_description: c.old_description.present().map(str::to_string),
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
