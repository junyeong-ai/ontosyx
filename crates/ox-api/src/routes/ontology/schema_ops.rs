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
pub async fn reindex_schema(
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

    let node_count = ontology.node_types.len();
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
pub async fn audit_graph(
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
pub async fn adopt_graph(
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
            nodes = ontology.node_types.len(),
            edges = ontology.edge_types.len(),
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
