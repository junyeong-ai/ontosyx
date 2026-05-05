use axum::Json;
use axum::extract::State;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use ox_ontology::ir::OntologyIR;
use ox_store::{OntologyRow, OntologyVersionSnapshot};

use crate::error::AppError;
use crate::principal::Principal;
use crate::response::ApiResponse;
use crate::state::AppState;

// ---------------------------------------------------------------------------
// Shared helpers — identity → current version → IR hydration.
//
// Every route on this module operates on "the ontology as it is right now":
// resolve the `ontologies` row, pick its current-valid version, hydrate the
// `OntologyIR` from the content-addressed entity store. The three steps are
// kept as one helper because each route needs the same failure semantics:
// `404` on unknown identity or vanished snapshot, `422` on
// present-but-unversioned, `500` on malformed stored entities.
// ---------------------------------------------------------------------------

async fn load_identity_current_ir(
    state: &AppState,
) -> Result<(OntologyRow, OntologyVersionSnapshot, OntologyIR), AppError> {
    let identity = state
        .store
        .get_workspace_ontology()
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Ontology"))?;
    let version = state
        .store
        .get_current_version(identity.id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::ontology_not_committed(identity.lineage_id.clone()))?;
    let ir = state
        .store
        .get_ontology_ir(version.id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Ontology version"))?;
    Ok((identity, version, ir))
}

use crate::routes::ontology_drafts::helpers::next_ontology_version_tag;

// ---------------------------------------------------------------------------
// POST /api/ontology/reindex — re-index schema embeddings
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/api/ontology/reindex",
    responses(
        (status = 200, description = "Re-indexing triggered", body = inline(ReindexResponse)),
        (status = 404, description = "Workspace has no ontology"),
    ),
    security(("api_key" = [])),
    tag = "Ontology",
)]
pub(crate) async fn reindex_schema(
    State(state): State<AppState>,
    _principal: Principal,
) -> Result<Json<ApiResponse<ReindexResponse>>, AppError> {
    let (identity, _, ontology) = load_identity_current_ir(&state).await?;

    let node_count = ontology.node_types().len();
    let memory = state
        .memory
        .as_ref()
        .ok_or_else(|| AppError::feature_not_configured("semantic_memory"))?;

    ox_brain::schema_rag::index_ontology_schema(memory, &ontology, &identity.id.to_string()).await;

    Ok(ApiResponse::of(ReindexResponse {
        ontology_id: identity.id,
        nodes_indexed: node_count,
    }))
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct ReindexResponse {
    pub ontology_id: Uuid,
    pub nodes_indexed: usize,
}

// ---------------------------------------------------------------------------
// POST /api/ontology/audit — compare ontology against live graph
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/api/ontology/audit",
    responses(
        (status = 200, description = "Audit report comparing ontology vs graph", body = Object),
        (status = 404, description = "Workspace has no ontology"),
        (status = 503, description = "Graph database not connected"),
    ),
    security(("api_key" = [])),
    tag = "Ontology",
)]
pub(crate) async fn graph_audit_report(
    State(state): State<AppState>,
    _principal: Principal,
) -> Result<Json<ApiResponse<ox_ontology::audit::GraphAuditReport>>, AppError> {
    let (_, _, ontology) = load_identity_current_ir(&state).await?;

    let runtime = state
        .runtime
        .as_ref()
        .ok_or_else(|| AppError::service_unavailable("Graph database not connected"))?;

    let overview =
        tokio::time::timeout(std::time::Duration::from_secs(10), runtime.graph_overview())
            .await
            .map_err(|_| AppError::internal("Graph overview timed out"))?
            .map_err(AppError::from)?;

    let report = ox_ontology::audit::audit_graph(&ontology, &overview);
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
    tag = "Ontology",
)]
pub(crate) async fn adopt_graph(
    State(state): State<AppState>,
    principal: Principal,
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
    let ontology =
        ox_ontology::audit::ontology_from_graph(&overview, &name).map_err(AppError::from)?;

    if req.save.unwrap_or(false) {
        // Persist as a new identity + initial version. `OntologyIR.id` is the
        // lineage handle external systems already reference, so we seed the
        // `ontologies.lineage_id` column from it rather than minting a fresh
        // UUID — keeps quality rules / saved queries pointing at the same
        // lineage string they would have seen under the legacy path.
        let description_json = serde_json::to_value(&ontology.description)
            .map_err(|e| AppError::internal(format!("Failed to serialize description: {e}")))?;
        let display_name_json = serde_json::to_value(&ontology.display_name)
            .map_err(|e| AppError::internal(format!("Failed to serialize display_name: {e}")))?;
        let lineage_seed = ontology.id.clone();
        let identity = state
            .store
            .create_ontology(&name, &display_name_json, &description_json, Some(&lineage_seed))
            .await
            .map_err(AppError::from)?;
        state
            .store
            .commit_version(
                identity.id,
                &ontology,
                "1",
                None,
                &principal.id,
                "Adopted from live graph",
            )
            .await
            .map_err(AppError::from)?;

        // Re-index schema embeddings for the committed ontology. Must use
        // `spawn_with_ws` so the spawned task inherits the workspace-scoped
        // task-locals that pgvector RLS depends on.
        if let Some(memory) = &state.memory {
            let memory = std::sync::Arc::clone(memory);
            let ont = ontology.clone();
            let identity_id = identity.id;
            let ws_scope = crate::spawn_scoped::WsScope::capture();
            crate::spawn_scoped::spawn_with_ws(ws_scope, async move {
                ox_brain::schema_rag::index_ontology_schema(&memory, &ont, &identity_id.to_string())
                    .await;
            });
        }

        tracing::info!(
            ontology_id = %identity.id,
            lineage_id = %identity.lineage_id,
            nodes = ontology.node_types().len(),
            edges = ontology.edge_types().len(),
            "Graph ontology adopted and committed"
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
// POST /api/ontologies/suggestions — proactive insight suggestions
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/api/ontologies/suggestions",
    request_body(content = Object, description = "OntologyIR to generate suggestions for"),
    responses(
        (status = 200, description = "List of insight suggestions", body = Vec<Object>),
    ),
    security(("api_key" = [])),
    tag = "Ontology",
)]
pub(crate) async fn suggest_insights(
    State(state): State<AppState>,
    _principal: Principal,
    Json(ontology): Json<OntologyIR>,
) -> Result<Json<ApiResponse<Vec<ox_ontology::InsightHint>>>, AppError> {
    let suggestions = state
        .brain
        .suggest_insights(&ontology, None)
        .await
        .map_err(AppError::from)?;
    Ok(ApiResponse::of(suggestions))
}

// ---------------------------------------------------------------------------
// POST /api/ontology/enrich — enrich descriptions with data samples
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
    path = "/api/ontology/enrich",
    request_body = EnrichRequest,
    responses(
        (status = 200, description = "Enrichment result", body = EnrichResponse),
    ),
    tag = "Ontology",
)]
pub(crate) async fn enrich_ontology(
    State(state): State<AppState>,
    principal: Principal,
    Json(req): Json<EnrichRequest>,
) -> Result<Json<ApiResponse<EnrichResponse>>, AppError> {
    let runtime = state.runtime.as_ref().ok_or_else(AppError::no_runtime)?;

    let (identity, current_version, ontology) = load_identity_current_ir(&state).await?;

    let config =
        ox_graph_runtime::profiler::ProfileConfig::for_ontology_size(ontology.node_types().len());
    let profile = ox_graph_runtime::profiler::profile_graph(runtime.as_ref(), &ontology, &config)
        .await
        .map_err(AppError::from)?;

    let profiled_nodes = profile.node_profiles.len();
    let profiled_edges = profile.edge_profiles.len();

    let result = ox_graph_runtime::enrichment::enrich_descriptions(&ontology, &profile);

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
        let next_tag = next_ontology_version_tag(&current_version.version);
        let commit_message = format!(
            "Enrichment: {} property description(s) updated",
            result.changes.len()
        );
        state
            .store
            .commit_version(
                identity.id,
                &result.ontology,
                &next_tag,
                Some(current_version.id),
                &principal.id,
                &commit_message,
            )
            .await
            .map_err(AppError::from)?;

        if let Some(memory) = &state.memory {
            let memory = std::sync::Arc::clone(memory);
            let identity_id = identity.id.to_string();
            let enriched = result.ontology.clone();
            crate::spawn_scoped::spawn_scoped(async move {
                ox_brain::schema_rag::index_ontology_schema(&memory, &enriched, &identity_id).await;
            });
        }

        tracing::info!(
            ontology_id = %identity.id,
            lineage_id = %identity.lineage_id,
            new_version = %next_tag,
            changes = changes.len(),
            "Ontology descriptions enriched with data samples"
        );
    }

    Ok(ApiResponse::of(EnrichResponse {
        ontology_id: identity.id,
        changes,
        profiled_nodes,
        profiled_edges,
        applied: req.apply,
    }))
}
