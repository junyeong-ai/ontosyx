use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    Json,
    extract::{FromRef, Path, Query, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use tracing::{error, info};
use uuid::Uuid;

use ox_ontology::ir::OntologyIR;
use ox_query_ir::pattern::PatternIR;
use ox_query_ir::query::{QueryIR, QueryResult};
use ox_core::types::PropertyValue;
use ox_runtime::cypher::{strict_advisory_diagnostics, strict_blocking_gate};
use ox_store::{CursorParams, QueryExecution, QueryExecutionSummary, SavedQueryPattern};

use crate::error::AppError;
use crate::principal::Principal;
use crate::response::ApiResponse;
use crate::state::AppState;
use crate::workspace::WorkspaceContext;

// ---------------------------------------------------------------------------
// Shared helpers — ontology injection for raw / IR paths
//
// `GRAPH_ONTOLOGY` drives the runtime's OntologyValidator. The agent path
// sets it from `DomainContext.ontology` automatically; raw HTTP paths
// opt in with a `ontology_id` so a power user who submits raw
// Cypher against a known ontology gets label-conformance checking for
// free. When no id is supplied, validation falls back to safety +
// workspace-scope only.
//
// `ontology_id` on the wire is interpreted as
// `ontologies.id` (Level 1 identity row). Each load walks identity →
// current version → hydrated IR through `OntologyVersionStore`.
// ---------------------------------------------------------------------------

/// Hydrate the current-version `OntologyIR` of the identity referenced by
/// `ontology_id`. Returns `None` iff `ontology_id` is `None`.
/// A present-but-unknown id yields 404; a present id whose lineage has no
/// committed version yields 422 — both expose the concrete failure to the
/// caller instead of silently falling back to unvalidated execution.
async fn load_ontology_current(
    state: &AppState,
    requested: Option<Uuid>,
) -> Result<Option<Arc<OntologyIR>>, AppError> {
    if requested.is_none() {
        return Ok(None);
    }
    // Workspace × ontology is 1:1; the caller's `ontology_id` is
    // the workspace's canonical by construction. We ignore the
    // bare value and resolve via the singleton accessor so the
    // request shape stays compatible without re-encoding the
    // implicit selection in the URL.
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
        .ok_or_else(|| {
            AppError::ontology_not_committed(identity.lineage_id.clone())
        })?;
    let ir = state
        .store
        .get_ontology_ir(version.id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Ontology version"))?;
    Ok(Some(Arc::new(ir)))
}

/// Resolve `query_ir.as_of` against stored ontology versions and
/// rewrite the query to the snapshot's label space. Leaves the
/// request untouched when no temporal pivot is present.
///
/// Requires `ontology_id` — the request must identify which
/// lineage to walk back through. Anonymous raw-IR queries (no saved
/// ontology) can't pivot because there's no version history to
/// consult; we reject with a clear 400 rather than silently compile
/// against an ontology that has nothing to do with the query's
/// actual schema.
async fn resolve_temporal(
    state: &AppState,
    mut req: ExecuteFromIrRequest,
) -> Result<ExecuteFromIrRequest, AppError> {
    let Some(as_of) = req.query_ir.as_of else {
        return Ok(req);
    };

    if req.ontology_id.is_none() {
        return Err(AppError::temporal_query_requires_ontology());
    }

    let identity = state
        .store
        .get_workspace_ontology()
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Ontology"))?;
    let lineage_id = identity.lineage_id.clone();

    // Current version — the label space the caller's QueryIR is authored in.
    let current_version = state
        .store
        .get_current_version(identity.id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::ontology_not_committed(lineage_id.clone()))?;
    let current = state
        .store
        .get_ontology_ir(current_version.id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Ontology version"))?;

    // Snapshot version — the bitemporally-live version at `as_of`.
    let snapshot_version = state
        .store
        .resolve_version_at(identity.id, as_of)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| {
            AppError::temporal_snapshot_missing(
                as_of.to_string(),
                lineage_id.clone(),
            )
        })?;
    let snapshot = state
        .store
        .get_ontology_ir(snapshot_version.id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| {
            AppError::temporal_snapshot_missing(
                as_of.to_string(),
                lineage_id.clone(),
            )
        })?;

    let rewritten =
        ox_compiler::rewrite_temporal_with_renames(req.query_ir, &snapshot, &current)
            .map_err(|e| AppError::query_compilation_failed(e.to_string()))?;
    req.query_ir = rewritten;
    Ok(req)
}

/// Run `fut` with `GRAPH_ONTOLOGY` bound to `ontology`, or unchanged if
/// none was supplied. Keeps call-sites free of `Option<Arc<_>>`-aware
/// branching.
async fn scope_with_ontology<F>(ontology: Option<Arc<OntologyIR>>, fut: F) -> F::Output
where
    F: std::future::Future,
{
    match ontology {
        Some(o) => ox_runtime::GRAPH_ONTOLOGY.scope(o, fut).await,
        None => fut.await,
    }
}

// ---------------------------------------------------------------------------
// POST /api/search — full-text search across Neo4j graph nodes
// ---------------------------------------------------------------------------

use ox_ontology::graph_exploration::SearchResultNode;

fn default_search_limit() -> usize {
    20
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct SearchGraphRequest {
    /// Search term to match against node properties.
    pub query: String,
    /// Max results (default 20, capped at 100).
    #[serde(default = "default_search_limit")]
    pub limit: usize,
    /// Optional label filter — only match nodes with these labels.
    pub labels: Option<Vec<String>>,
}

#[utoipa::path(
    post,
    path = "/api/search",
    request_body = SearchGraphRequest,
    responses(
        (status = 200, description = "Search results", body = Vec<SearchResultNode>),
        (status = 400, description = "Empty query", body = inline(crate::openapi::ErrorResponse)),
        (status = 503, description = "Graph database not connected", body = inline(crate::openapi::ErrorResponse)),
        (status = 504, description = "Query timeout", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Query",
)]
pub(crate) async fn search_graph(
    State(state): State<AppState>,
    principal: Principal,
    _ws: WorkspaceContext,
    Json(req): Json<SearchGraphRequest>,
) -> Result<Json<ApiResponse<Vec<ox_ontology::graph_exploration::SearchResultNode>>>, AppError> {
    let search_term = req.query.trim().to_string();
    if search_term.is_empty() {
        return Err(AppError::query_text_empty());
    }

    let limit = req.limit.min(100);
    info!(user_id = %principal.id, query = %search_term, limit, "Graph search");

    let runtime = state.runtime.as_ref().ok_or_else(AppError::no_runtime)?;
    let timeout = state.timeouts.raw_query;
    let labels = req.labels.as_deref();

    let results = tokio::time::timeout(
        timeout,
        runtime.search_nodes(&search_term, limit, labels),
    )
    .await
    .map_err(|_| AppError::timeout(format!("Search timed out after {}s", timeout.as_secs())))?
    .map_err(|e| {
        error!("Graph search failed: {e}");
        AppError::query_execution_failed(e.to_string())
    })?;

    Ok(ApiResponse::of(results))
}

// ---------------------------------------------------------------------------
// POST /api/query/raw — direct query execution (power users)
//
// Accepts a raw query in the target language (e.g., Cypher).
// Skips NL translation — zero LLM calls.
// ---------------------------------------------------------------------------

#[derive(Deserialize, utoipa::ToSchema)]
pub struct ExecuteRawQueryRequest {
    /// Raw query statement in the target language (e.g., Cypher).
    pub query: String,
    /// Optional saved-ontology id. When present, the runtime's
    /// OntologyValidator gates the query against this ontology (rejects
    /// unknown labels / relationship types / property keys). When
    /// omitted, the raw path stays ontology-free and only the safety
    /// + workspace-scope gates apply.
    #[serde(default)]
    pub ontology_id: Option<Uuid>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct ExecuteRawQueryResponse {
    /// The query that was executed.
    pub query: String,
    /// Compiler target language (e.g., "cypher").
    pub target: String,
    /// Query result rows and metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<Object>)]
    pub results: Option<QueryResult>,
}

#[utoipa::path(
    post,
    path = "/api/query/raw",
    request_body = ExecuteRawQueryRequest,
    responses(
        (status = 200, description = "Raw query result", body = ExecuteRawQueryResponse),
        (status = 400, description = "Empty query", body = inline(crate::openapi::ErrorResponse)),
        (status = 422, description = "Query execution failed", body = inline(crate::openapi::ErrorResponse)),
        (status = 503, description = "Graph database not connected", body = inline(crate::openapi::ErrorResponse)),
        (status = 504, description = "Query timeout", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Query",
)]
#[tracing::instrument(skip(state, principal, req))]
pub(crate) async fn raw_query(
    State(state): State<AppState>,
    principal: Principal,
    ws: WorkspaceContext,
    Json(req): Json<ExecuteRawQueryRequest>,
) -> Result<Json<ApiResponse<ExecuteRawQueryResponse>>, AppError> {
    if req.query.trim().is_empty() {
        return Err(AppError::query_text_empty());
    }

    // Block write operations unless user has designer role
    let upper = req.query.to_uppercase();
    const WRITE_KEYWORDS: &[&str] = &["DELETE", "DETACH", "CREATE", "MERGE", "SET ", "REMOVE "];
    let has_write = WRITE_KEYWORDS.iter().any(|kw| upper.contains(kw));
    if has_write {
        principal.require_designer()?;
    }

    let target = state.compiler.name().to_string();
    info!(user_id = %principal.id, target = %target, "Raw query submitted");

    // Pre-execute blocking gate — refuses Cartesian products and
    // destructive-write smells before they hit the driver. The
    // existing safety pipeline catches outright forbidden tokens;
    // this catches the smaller set of patterns that are syntactically
    // valid but semantically dangerous.
    strict_blocking_gate(&req.query, &ws.workspace_id.to_string())
        .map_err(AppError::from)?;

    let runtime = state.runtime.as_ref().ok_or_else(AppError::no_runtime)?;

    // If the caller supplied an ontology id, load it so the runtime's
    // OntologyValidator can reject unknown labels before they hit the
    // driver. Raw path is opt-in: no id → ontology gate is skipped
    // (safety + workspace-scope still apply).
    let ontology = load_ontology_current(&state, req.ontology_id).await?;

    // Π-3 pre-fetch. Raw path has no QueryIR to walk for `type_ids` /
    // `filter_summary` — the identity + version pair is still useful
    // for response attribution, so we capture those before execution
    // and stamp them onto the result metadata below.
    let ontology_version = if let Some(id) = req.ontology_id {
        state
            .store
            .get_current_version(id)
            .await
            .map_err(AppError::from)?
            .map(|v| v.version)
    } else {
        None
    };

    let timeout = state.timeouts.raw_query;
    let empty_params: HashMap<String, PropertyValue> = HashMap::new();
    let start = std::time::Instant::now();
    let exec_fut = runtime.execute_query(&req.query, &empty_params);
    let mut results = tokio::time::timeout(
        timeout,
        scope_with_ontology(ontology.clone(), exec_fut),
    )
    .await
    .map_err(|_| {
        crate::metrics::record_query("timeout", start.elapsed());
        AppError::timeout(format!(
            "Query execution timed out after {}s",
            timeout.as_secs()
        ))
    })?
    .map_err(|e| {
        crate::metrics::record_query("error", start.elapsed());
        error!("Raw query execution failed: {e}");
        AppError::query_execution_failed(e.to_string())
    })?;
    crate::metrics::record_query("ok", start.elapsed());

    // Π-3 provenance — raw path. `type_ids` / `filter_summary` are
    // intentionally empty here: we have no QueryIR to walk, and
    // substituting the raw statement would leak free-form text the
    // LLM/admin UI cannot trust as structured provenance. The
    // identity + version pair (when supplied) is still the right
    // handle for "which schema did this run against".
    if req.ontology_id.is_some() {
        results.metadata.provenance = Some(ox_query_ir::query::QueryProvenance {
            ontology_id: req.ontology_id.map(|id| id.to_string()),
            ontology_version,
            as_of: None,
            source_ids: Vec::new(),
            type_ids: Vec::new(),
            filter_summary: None,
            registry_versions: std::collections::BTreeMap::new(),
            column_lineage: Vec::new(),
        });
    }

    // Advisory diagnostics — strict-mode revalidation of the executed
    // Cypher. The runtime's permissive pass let the query through; this
    // pass captures the warnings the user should see.
    results.metadata.warnings = strict_advisory_diagnostics(&req.query, &ws.workspace_id.to_string());

    // Record metering (fire-and-forget)
    let execution_time_ms = start.elapsed().as_millis() as i64;
    let row_count = results.metadata.rows_returned;
    {
        let meter_store = Arc::clone(&state.store);
        let meter_user = principal.user_uuid().ok();
        crate::spawn_scoped::spawn_scoped(async move {
            if let Err(error) = meter_store
                .record_usage(
                    meter_user,
                    "query",
                    None,
                    None,
                    Some("raw_query"),
                    0,
                    0,
                    execution_time_ms,
                    0.0,
                    serde_json::json!({"rows": row_count}),
                )
                .await {
                tracing::warn!(?error, "telemetry record failed");
            }
        });
    }

    Ok(ApiResponse::of(ExecuteRawQueryResponse {
        query: req.query,
        target,
        results: Some(results),
    }))
}

// ---------------------------------------------------------------------------
// GET /api/query/history — list past query executions (cursor-paginated)
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/query/history",
    params(
        ("limit" = Option<u32>, Query, description = "Max items to return (default 50, max 100)"),
        ("cursor" = Option<String>, Query, description = "Opaque cursor from a previous response"),
    ),
    responses(
        (status = 200, description = "Paginated query execution history", body = Object),
    ),
    tag = "Query",
)]
pub(crate) async fn list_executions(
    State(state): State<AppState>,
    principal: Principal,
    Query(params): Query<CursorParams>,
) -> Result<Json<ApiResponse<Vec<QueryExecutionSummary>>>, AppError> {
    let page = state
        .store
        .list_query_executions(&principal.id, &params)
        .await?;
    Ok(ApiResponse::page(page))
}

// ---------------------------------------------------------------------------
// GET /api/query/history/:id — get a single query execution
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/query/history/{id}",
    params(
        ("id" = Uuid, Path, description = "Query execution ID"),
    ),
    responses(
        (status = 200, description = "Query execution details", body = Object),
        (status = 404, description = "Execution not found", body = inline(crate::openapi::ErrorResponse)),
    ),
    tag = "Query",
)]
pub(crate) async fn get_execution(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<QueryExecution>>, AppError> {
    let execution = state
        .store
        .get_query_execution(&principal.id, id)
        .await?
        .ok_or_else(AppError::execution_not_found)?;
    Ok(ApiResponse::of(execution))
}

// ---------------------------------------------------------------------------
// PATCH /api/query/history/:id/feedback — submit accuracy feedback
// ---------------------------------------------------------------------------

#[derive(Deserialize, utoipa::ToSchema)]
pub struct SubmitQueryFeedbackRequest {
    /// "positive", "negative", or null to clear feedback
    pub feedback: Option<String>,
}

const VALID_FEEDBACK: &[&str] = &["positive", "negative"];

#[utoipa::path(
    patch,
    path = "/api/query/history/{id}/feedback",
    params(("id" = Uuid, Path, description = "Query execution ID")),
    request_body = SubmitQueryFeedbackRequest,
    responses(
        (status = 200, description = "Feedback recorded"),
        (status = 400, description = "Invalid feedback value"),
        (status = 404, description = "Execution not found"),
    ),
    security(("bearer" = [])),
    tag = "Query",
)]
pub(crate) async fn update_feedback(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
    Json(req): Json<SubmitQueryFeedbackRequest>,
) -> Result<StatusCode, AppError> {
    if let Some(ref fb) = req.feedback
        && !VALID_FEEDBACK.contains(&fb.as_str())
    {
        return Err(AppError::invalid_enum_value(
            "feedback",
            fb.clone(),
            VALID_FEEDBACK,
        ));
    }

    let updated = state
        .store
        .update_query_feedback(id, &principal.id, req.feedback.as_deref())
        .await
        .map_err(AppError::from)?;

    if !updated {
        return Err(AppError::execution_not_found());
    }

    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// POST /api/query/from-ir — execute a query from QueryIR JSON
//
// Used by the visual query builder. Compiles QueryIR → target language
// (e.g. Cypher) → executes → returns results with a widget hint.
// ---------------------------------------------------------------------------

#[derive(Deserialize, utoipa::ToSchema)]
pub struct ExecuteFromIrRequest {
    /// The QueryIR to compile and execute.
    #[schema(value_type = Object)]
    pub query_ir: QueryIR,
    /// Optional saved-ontology id. When present, the OntologyValidator
    /// gates the compiled Cypher against this ontology so a canvas built
    /// on a stale schema gets rejected with a precise "unknown label"
    /// error instead of a generic driver failure.
    #[serde(default)]
    pub ontology_id: Option<Uuid>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct ExecuteFromIrResponse {
    /// The compiled query statement in the target language.
    pub compiled_query: String,
    /// The compilation target (e.g. "cypher").
    pub compiled_target: String,
    /// Query result rows and metadata.
    #[schema(value_type = Object)]
    pub result: QueryResult,
    /// Widget hint for optimal result visualization.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub widget_hint: Option<serde_json::Value>,
}

#[utoipa::path(
    post,
    path = "/api/query/from-ir",
    request_body = ExecuteFromIrRequest,
    responses(
        (status = 200, description = "Compiled and executed query result", body = ExecuteFromIrResponse),
        (status = 400, description = "Invalid QueryIR", body = inline(crate::openapi::ErrorResponse)),
        (status = 422, description = "Compilation or execution failed", body = inline(crate::openapi::ErrorResponse)),
        (status = 503, description = "Graph database not connected", body = inline(crate::openapi::ErrorResponse)),
        (status = 504, description = "Query timeout", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Query",
)]
#[tracing::instrument(skip(state, principal, req))]
pub(crate) async fn execute_from_ir(
    State(state): State<AppState>,
    principal: Principal,
    ws: WorkspaceContext,
    Json(req): Json<ExecuteFromIrRequest>,
) -> Result<Json<ApiResponse<ExecuteFromIrResponse>>, AppError> {
    let target = state.compiler.name().to_string();
    info!(user_id = %principal.id, target = %target, "QueryIR execution submitted");

    // Capture the raw `as_of` before the temporal rewriter consumes it —
    // Π-3 provenance needs the original pivot for the response summary.
    let original_as_of = req.query_ir.as_of;

    // Step 0: Resolve the temporal pivot, if any. `query_ir.as_of` asks
    // the compiler to evaluate the query as it would have run at a past
    // timestamp — we load the ontology snapshot that was live then,
    // rewrite any renamed labels current→snapshot, and hand the
    // compiler a QueryIR with as_of cleared.
    //
    // Surfaces three distinct failure modes so the UI can present them
    // individually: (a) no ontology_id supplied, (b) the lineage
    // predates the requested timestamp, (c) window / rename
    // inconsistency from the rewriter itself.
    let mut req = resolve_temporal(&state, req).await?;

    // Step 0.5: Auto-DISTINCT pass. When the caller supplied an
    // ontology id we load it here and rewrite any aggregation that
    // crosses a OneToMany / ManyToMany link so every AggregationExpr
    // carries `distinct: true`. Without this pass a query like
    // `MATCH (a)-[:HAS_MANY]->(b) RETURN sum(b.value)` silently
    // double-counts when the physical mapping is a fan-out join.
    // (Π-2.) Idempotent — re-running on an already-rewritten IR is
    // a no-op, which matters because a client that pre-set
    // `distinct: true` stays unchanged.
    let ontology = load_ontology_current(&state, req.ontology_id).await?;
    if let Some(ont) = ontology.as_ref() {
        req.query_ir = ox_compiler::rewrite_auto_distinct(req.query_ir, ont);
    }

    // Fetch the committed version tag for Π-3 provenance. Cheap — hits
    // the partial `ontology_version_snapshots_current_idx`. `None` when
    // no ontology_id is supplied.
    let ontology_version = if let Some(id) = req.ontology_id {
        state
            .store
            .get_current_version(id)
            .await
            .map_err(AppError::from)?
            .map(|v| v.version)
    } else {
        None
    };

    // Step 1: Compile QueryIR → target language. The compiler applies
    // ConceptMap rewrite internally when an ontology is supplied;
    // raw-QueryIR callers (no `ontology_id`) opt out by passing None.
    let compiled = state
        .compiler
        .compile_query(&req.query_ir, ontology.as_deref())
        .map_err(|e| {
            error!("QueryIR compilation failed: {e}");
            AppError::query_compilation_failed(e.to_string())
        })?;

    // Pre-execute blocking gate on the compiled Cypher — even when
    // the IR was authored cleanly, the lowering may have introduced
    // a Cartesian or other dangerous shape; gate it before the
    // driver call.
    strict_blocking_gate(&compiled.statement, &ws.workspace_id.to_string())
        .map_err(AppError::from)?;

    // Step 2: Execute the compiled query
    let runtime = state.runtime.as_ref().ok_or_else(AppError::no_runtime)?;

    let timeout = state.timeouts.raw_query;
    let start = std::time::Instant::now();
    let exec_fut = runtime.execute_query(&compiled.statement, &compiled.params);
    let mut results = tokio::time::timeout(
        timeout,
        scope_with_ontology(ontology.clone(), exec_fut),
    )
    .await
    .map_err(|_| {
        crate::metrics::record_query("timeout", start.elapsed());
        AppError::timeout(format!(
            "Query execution timed out after {}s",
            timeout.as_secs()
        ))
    })?
    .map_err(|e| {
        crate::metrics::record_query("error", start.elapsed());
        error!("QueryIR execution failed: {e}");
        AppError::query_execution_failed(e.to_string())
    })?;
    crate::metrics::record_query("ok", start.elapsed());

    // Step 3: Auto-detect best widget type (non-blocking, best-effort)
    let widget_hint = if results.metadata.rows_returned > 0 {
        let sample = serde_json::to_string(&results.rows.iter().take(5).collect::<Vec<_>>())
            .unwrap_or_default();
        match state.brain.select_widget(&req.query_ir, &sample).await {
            Ok(hint) => serde_json::to_value(&hint).ok(),
            Err(e) => {
                tracing::warn!("Widget hint selection failed: {e}");
                None
            }
        }
    } else {
        None
    };

    // Advisory diagnostics: strict revalidation of the compiled Cypher.
    // Non-blocking — the runtime already let the query through via the
    // permissive validator pass.
    results.metadata.warnings =
        strict_advisory_diagnostics(&compiled.statement, &ws.workspace_id.to_string());

    // Π-3 provenance — stamp the response envelope with the ontology
    // identity / version / temporal pivot / touched types / filter
    // summary the LLM + admin UI need to explain where the numbers
    // came from. `source_ids` stays empty on the Cypher path (graph
    // runtime owns a single backend); the federation handler below
    // populates it from the plan's resolver set.
    results.metadata.provenance = Some(ox_compiler::build_provenance(
        &req.query_ir,
        &ox_compiler::ProvenanceContext {
            ontology_id: req.ontology_id.map(|id| id.to_string()),
            ontology_version,
            as_of: original_as_of,
            source_ids: Vec::new(),
            ontology: ontology.as_deref(),
        },
    ));

    // Record metering (fire-and-forget)
    let execution_time_ms = start.elapsed().as_millis() as i64;
    let row_count = results.metadata.rows_returned;
    {
        let meter_store = Arc::clone(&state.store);
        let meter_user = principal.user_uuid().ok();
        crate::spawn_scoped::spawn_scoped(async move {
            if let Err(error) = meter_store
                .record_usage(
                    meter_user,
                    "query",
                    None,
                    None,
                    Some("from_ir"),
                    0,
                    0,
                    execution_time_ms,
                    0.0,
                    serde_json::json!({"rows": row_count}),
                )
                .await {
                tracing::warn!(?error, "telemetry record failed");
            }
        });
    }

    Ok(ApiResponse::of(ExecuteFromIrResponse {
        compiled_query: compiled.statement,
        compiled_target: target,
        result: results,
        widget_hint,
    }))
}

// ---------------------------------------------------------------------------
// POST /api/query/from-ir/federation — execute a QueryIR via the
// virtual-ontology-layer (VOL) federation engine instead of the
// Cypher / Neo4j path.
//
// The planner walks the OntologyIR referenced by `ontology_id`,
// resolves every `ObjectMappingDef.source_id` through the per-request
// `AppState::federation_resolver`, and emits a DataFusion LogicalPlan
// that scans the registered adapters directly. Results are projected
// from Arrow RecordBatches into the standard `QueryResult` shape so
// downstream tooling (widget selector, ACL pass) works unchanged.
//
// `ontology_id` is required on this path — unlike the Cypher
// handler (where an unknown label can at worst surface as a driver
// error), the federation planner has no fallback when it cannot map
// a label to a node type.
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/api/query/from-ir/federation",
    request_body = ExecuteFromIrRequest,
    responses(
        (
            status = 200,
            description = "Federation-executed query result",
            body = ExecuteFromIrResponse
        ),
        (
            status = 400,
            description = "Missing ontology_id or invalid QueryIR",
            body = inline(crate::openapi::ErrorResponse)
        ),
        (
            status = 422,
            description = "Federation planning or execution failed",
            body = inline(crate::openapi::ErrorResponse)
        ),
    ),
    security(("api_key" = [])),
    tag = "Query",
)]
#[tracing::instrument(skip(state, principal, req))]
pub(crate) async fn execute_from_ir_federation(
    State(state): State<AppState>,
    principal: Principal,
    ws: WorkspaceContext,
    Json(req): Json<ExecuteFromIrRequest>,
) -> Result<Json<ApiResponse<ExecuteFromIrResponse>>, AppError> {
    info!(user_id = %principal.id, "federation QueryIR execution submitted");

    let ontology_id = req
        .ontology_id
        .ok_or_else(|| AppError::required_field_empty("ontology_id"))?;

    // Capture as_of before temporal rewrite consumes it (Π-3).
    let original_as_of = req.query_ir.as_of;

    // Temporal rewriting is identical to the Cypher path — the label
    // renames get applied before planning so the federation planner
    // sees the snapshot's label space.
    let mut req = resolve_temporal(&state, req).await?;

    let ontology = load_ontology_current(&state, Some(ontology_id))
        .await?
        .ok_or_else(|| AppError::not_found("Ontology"))?;

    // Π-2 auto-DISTINCT. The federation planner (DataFusion-backed)
    // inherits the same fan-out risk as the Cypher compiler when an
    // aggregation traverses a OneToMany / ManyToMany edge, so the
    // same pre-pass runs on this path. Idempotent — a client can
    // always override by setting `distinct: false` after this rewrite
    // lands, but the default now matches schema semantics.
    req.query_ir = ox_compiler::rewrite_auto_distinct(req.query_ir, ontology.as_ref());

    // Π-3 provenance pre-fetch — same pattern as the Cypher path.
    let ontology_version = state
        .store
        .get_current_version(ontology_id)
        .await
        .map_err(AppError::from)?
        .map(|v| v.version);

    let start = std::time::Instant::now();

    // Look up — or lazily hydrate from the data_sources store —
    // the workspace's federation resolver. `ensure_workspace_resolver`
    // takes the narrowed `FederationState`, which extracts cleanly
    // from the full `AppState` this handler holds. The helper
    // returns an owned clone so we don't hold the outer map lock
    // across the planner's async `describe_table` calls.
    let federation_state = crate::state::FederationState::from_ref(&state);
    let resolver =
        crate::federation_resolver::ensure_workspace_resolver(&federation_state, ws.workspace_id)
            .await?;
    let plan = ox_federation::build_query_ir_scoped(
        &ontology,
        &req.query_ir,
        &ws.workspace_id.to_string(),
        &resolver,
    )
    .await
    .map_err(|e| {
        crate::metrics::record_query("error", start.elapsed());
        error!("Federation planning failed: {e}");
        AppError::query_compilation_failed(e.to_string())
    })?;

    // Preserve the plan's EXPLAIN-style rendering as `compiled_query`
    // for parity with the Cypher response shape. Clients that
    // previously rendered Cypher get a DataFusion logical plan here
    // — useful for debugging and for showing the user *what ran*.
    let compiled_display = format!("{plan}");

    let ctx = ox_federation::FederationContext::new(
        ox_federation::context::WorkspaceRef::new(ws.workspace_id.to_string()),
    );
    let batches = ctx.execute_plan(plan).await.map_err(|e| {
        crate::metrics::record_query("error", start.elapsed());
        error!("Federation execution failed: {e}");
        AppError::query_execution_failed(e.to_string())
    })?;
    crate::metrics::record_query("ok", start.elapsed());

    let elapsed_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
    let mut results = crate::arrow_conversion::record_batches_to_query_result(&batches, elapsed_ms)
        .map_err(|e| AppError::internal(format!("arrow conversion: {e}")))?;

    // ACL enforcement on the federation path — DataFusion executes
    // outside the Cypher pipeline, so the rewriter never sees the
    // plan. Read the request-scoped snapshot the middleware loaded
    // and apply it post-hoc to the materialised result.
    if let Ok(snapshot) = ox_runtime::GRAPH_ACL_SNAPSHOT.try_with(std::sync::Arc::clone) {
        crate::acl_enforcement::enforce_acl_on_result(&mut results, &snapshot);
    }

    // Advisory diagnostics: the federation path executes a DataFusion
    // LogicalPlan, not Cypher, so the Cypher-specific validators
    // (complexity, semantic-guard) don't apply. A future DataFusion
    // complexity gate would drop in here; for now the field stays
    // empty on the federation path so the frontend treats it as
    // "no warnings" rather than "validator didn't run".
    results.metadata.warnings = Vec::new();

    // Π-3 provenance — `source_ids` is left empty here and filled in
    // by `build_provenance` from the ontology's ObjectMappingDef /
    // LinkMappingDef declarations. This is strictly tighter than
    // "every adapter registered on this workspace" — only the
    // sources the ontology says the query's labels can reach
    // contribute. Follow-up Π-2 (LogicalPlan inspection for the
    // exact scans the planner chose) would further narrow this; the
    // IR-level set is the right default until that lands.
    results.metadata.provenance = Some(ox_compiler::build_provenance(
        &req.query_ir,
        &ox_compiler::ProvenanceContext {
            ontology_id: Some(ontology_id.to_string()),
            ontology_version,
            as_of: original_as_of,
            source_ids: Vec::new(),
            ontology: Some(ontology.as_ref()),
        },
    ));

    let row_count = results.metadata.rows_returned;
    let execution_time_ms = i64::try_from(elapsed_ms).unwrap_or(i64::MAX);
    {
        let meter_store = Arc::clone(&state.store);
        let meter_user = principal.user_uuid().ok();
        crate::spawn_scoped::spawn_scoped(async move {
            if let Err(error) = meter_store
                .record_usage(
                    meter_user,
                    "query",
                    None,
                    None,
                    Some("from_ir_federation"),
                    0,
                    0,
                    execution_time_ms,
                    0.0,
                    serde_json::json!({"rows": row_count}),
                )
                .await {
                tracing::warn!(?error, "telemetry record failed");
            }
        });
    }

    Ok(ApiResponse::of(ExecuteFromIrResponse {
        compiled_query: compiled_display,
        compiled_target: "datafusion/logical-plan".to_string(),
        result: results,
        widget_hint: None,
    }))
}

// ---------------------------------------------------------------------------
// POST /api/query/pattern/compile — lower a canvas PatternIR to QueryIR
//
// Pure transformation — no DB, no LLM. The visual query builder calls this
// after the user finishes editing to produce the compiled QueryIR that
// `/api/query/from-ir` will execute.
// ---------------------------------------------------------------------------

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CompilePatternRequest {
    /// The canvas PatternIR to lower.
    #[schema(value_type = Object)]
    pub pattern_ir: PatternIR,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct CompilePatternResponse {
    /// The lowered QueryIR, ready to execute via `/api/query/from-ir`.
    #[schema(value_type = Object)]
    pub query_ir: QueryIR,
}

#[utoipa::path(
    post,
    path = "/api/query/pattern/compile",
    request_body = CompilePatternRequest,
    responses(
        (status = 200, description = "Compiled QueryIR", body = CompilePatternResponse),
    ),
    security(("api_key" = [])),
    tag = "Query",
)]
pub(crate) async fn compile_pattern(
    _principal: Principal,
    _ws: WorkspaceContext,
    Json(req): Json<CompilePatternRequest>,
) -> Result<Json<ApiResponse<CompilePatternResponse>>, AppError> {
    let query_ir = req.pattern_ir.compile().map_err(AppError::from)?;
    Ok(ApiResponse::of(CompilePatternResponse { query_ir }))
}

// ---------------------------------------------------------------------------
// POST /api/query/pattern/decompile — reconstruct a canvas view from a QueryIR
//
// Pure transformation — no DB, no LLM. Best-effort: non-Match operations
// (PathFind, Union, Chain, …) yield an empty PatternIR. The UI detects
// that shape and shows "this query can't be edited visually" rather than
// a blank canvas the user will mistake for a fresh query.
// ---------------------------------------------------------------------------

#[derive(Deserialize, utoipa::ToSchema)]
pub struct DecompilePatternRequest {
    /// The QueryIR to reconstruct onto the canvas.
    #[schema(value_type = Object)]
    pub query_ir: QueryIR,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct DecompilePatternResponse {
    /// The reconstructed PatternIR. Positions are always `None` — the
    /// UI runs its own layout pass before rendering.
    #[schema(value_type = Object)]
    pub pattern_ir: PatternIR,
    /// `true` iff the source QueryIR was a `Match` operation and
    /// therefore fully representable on the canvas. `false` for
    /// PathFind / Union / Chain / Aggregate / CallSubquery / Mutate /
    /// Analytics — the UI should surface a "read-only" indicator.
    pub editable: bool,
}

#[utoipa::path(
    post,
    path = "/api/query/pattern/decompile",
    request_body = DecompilePatternRequest,
    responses(
        (status = 200, description = "Reconstructed PatternIR", body = DecompilePatternResponse),
    ),
    security(("api_key" = [])),
    tag = "Query",
)]
pub(crate) async fn decompile_pattern(
    _principal: Principal,
    _ws: WorkspaceContext,
    Json(req): Json<DecompilePatternRequest>,
) -> Result<Json<ApiResponse<DecompilePatternResponse>>, AppError> {
    let editable = matches!(
        req.query_ir.operation,
        ox_query_ir::query::QueryOp::Match { .. }
    );
    let pattern_ir = PatternIR::decompile(&req.query_ir);
    Ok(ApiResponse::of(DecompilePatternResponse {
        pattern_ir,
        editable,
    }))
}

// ---------------------------------------------------------------------------
// Saved PatternIR — canvas layout persistence
//
// `/pattern/compile` is intentionally canvas-agnostic (drops positions and
// zoom). This resource persists the *PatternIR itself* so reopening a
// saved query restores the user's node layout, pan, and zoom without a
// second layout pass.
// ---------------------------------------------------------------------------

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateSavedPatternRequest {
    /// Unique name within (user, ontology) — UI uses it as a handle.
    pub name: String,
    /// Optional free-form note.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Ontology the pattern was built against. The saved pattern is tied
    /// to an ontology; reopening against a different ontology requires
    /// the caller to decide how to reconcile unknown labels.
    pub ontology_lineage_id: String,
    /// The full PatternIR — nodes with positions, edges, filters, and
    /// layout hints (zoom + pan). QueryIR is computed on demand from
    /// `pattern_ir.compile()` and does not need to be stored.
    #[schema(value_type = Object)]
    pub pattern_ir: PatternIR,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct UpdateSavedPatternRequest {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[schema(value_type = Object)]
    pub pattern_ir: PatternIR,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct SavedPatternResponse {
    pub id: Uuid,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub ontology_lineage_id: String,
    #[schema(value_type = Object)]
    pub pattern_ir: PatternIR,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl SavedPatternResponse {
    fn try_from_row(row: SavedQueryPattern) -> Result<Self, AppError> {
        let pattern_ir: PatternIR = serde_json::from_value(row.pattern_ir)
            .map_err(|e| AppError::internal(format!("deserialize pattern_ir: {e}")))?;
        Ok(Self {
            id: row.id,
            name: row.name,
            description: row.description,
            ontology_lineage_id: row.ontology_lineage_id,
            pattern_ir,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

#[utoipa::path(
    post,
    path = "/api/query/pattern/saved",
    request_body = CreateSavedPatternRequest,
    responses(
        (status = 200, description = "Saved pattern created", body = SavedPatternResponse),
        (status = 409, description = "Name already exists for (user, ontology)"),
    ),
    security(("api_key" = [])),
    tag = "Query",
)]
pub(crate) async fn create_saved_pattern(
    State(state): State<AppState>,
    principal: Principal,
    _ws: WorkspaceContext,
    Json(req): Json<CreateSavedPatternRequest>,
) -> Result<Json<ApiResponse<SavedPatternResponse>>, AppError> {
    let pattern_ir_json = serde_json::to_value(&req.pattern_ir)
        .map_err(|e| AppError::internal(format!("serialize pattern_ir: {e}")))?;
    let now = chrono::Utc::now();
    let row = SavedQueryPattern {
        id: Uuid::new_v4(),
        user_id: principal.id.clone(),
        ontology_lineage_id: req.ontology_lineage_id,
        name: req.name,
        description: req.description,
        pattern_ir: pattern_ir_json,
        created_at: now,
        updated_at: now,
    };
    state
        .store
        .create_pattern(&row)
        .await
        .map_err(AppError::from)?;
    Ok(ApiResponse::of(SavedPatternResponse::try_from_row(row)?))
}

#[derive(Deserialize, utoipa::IntoParams)]
pub struct ListSavedPatternsParams {
    pub ontology_lineage_id: String,
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/query/pattern/saved",
    params(ListSavedPatternsParams),
    responses(
        (status = 200, description = "Paginated saved patterns", body = Object),
    ),
    security(("api_key" = [])),
    tag = "Query",
)]
pub(crate) async fn list_saved_patterns(
    State(state): State<AppState>,
    principal: Principal,
    _ws: WorkspaceContext,
    Query(params): Query<ListSavedPatternsParams>,
) -> Result<Json<ApiResponse<Vec<SavedPatternResponse>>>, AppError> {
    let pagination = CursorParams {
        limit: params.limit.unwrap_or(50),
        cursor: params.cursor,
    };
    let page = state
        .store
        .list_patterns(&principal.id, &params.ontology_lineage_id, &pagination)
        .await
        .map_err(AppError::from)?;
    let items = page
        .items
        .into_iter()
        .map(SavedPatternResponse::try_from_row)
        .collect::<Result<Vec<_>, AppError>>()?;
    Ok(ApiResponse::page(ox_store::CursorPage {
        items,
        next_cursor: page.next_cursor,
    }))
}

#[utoipa::path(
    get,
    path = "/api/query/pattern/saved/{id}",
    params(("id" = Uuid, Path, description = "Saved pattern ID")),
    responses(
        (status = 200, description = "Saved pattern", body = SavedPatternResponse),
        (status = 404, description = "Not found"),
    ),
    security(("api_key" = [])),
    tag = "Query",
)]
pub(crate) async fn get_saved_pattern(
    State(state): State<AppState>,
    _principal: Principal,
    _ws: WorkspaceContext,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<SavedPatternResponse>>, AppError> {
    let row = state
        .store
        .get_pattern(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Saved pattern"))?;
    Ok(ApiResponse::of(SavedPatternResponse::try_from_row(row)?))
}

#[utoipa::path(
    patch,
    path = "/api/query/pattern/saved/{id}",
    params(("id" = Uuid, Path, description = "Saved pattern ID")),
    request_body = UpdateSavedPatternRequest,
    responses(
        (status = 204, description = "Updated"),
        (status = 404, description = "Not found"),
    ),
    security(("api_key" = [])),
    tag = "Query",
)]
pub(crate) async fn update_saved_pattern(
    State(state): State<AppState>,
    _principal: Principal,
    _ws: WorkspaceContext,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateSavedPatternRequest>,
) -> Result<StatusCode, AppError> {
    let pattern_ir_json = serde_json::to_value(&req.pattern_ir)
        .map_err(|e| AppError::internal(format!("serialize pattern_ir: {e}")))?;
    let updated = state
        .store
        .update_pattern(id, &req.name, req.description.as_deref(), &pattern_ir_json)
        .await
        .map_err(AppError::from)?;
    if !updated {
        return Err(AppError::not_found("Saved pattern"));
    }
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    delete,
    path = "/api/query/pattern/saved/{id}",
    params(("id" = Uuid, Path, description = "Saved pattern ID")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 404, description = "Not found"),
    ),
    security(("api_key" = [])),
    tag = "Query",
)]
pub(crate) async fn delete_saved_pattern(
    State(state): State<AppState>,
    _principal: Principal,
    _ws: WorkspaceContext,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let deleted = state
        .store
        .delete_pattern(id)
        .await
        .map_err(AppError::from)?;
    if !deleted {
        return Err(AppError::not_found("Saved pattern"));
    }
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// GET /api/graph/overview — graph schema overview (delegated to GraphRuntime)
// ---------------------------------------------------------------------------

use ox_ontology::graph_exploration::GraphSchemaOverview;

#[utoipa::path(
    get,
    path = "/api/graph/overview",
    responses(
        (status = 200, description = "Graph schema overview", body = GraphSchemaOverview),
        (status = 503, description = "Graph database not connected"),
        (status = 504, description = "Timeout"),
    ),
    security(("api_key" = [])),
    tag = "Query",
)]
pub(crate) async fn graph_overview(
    State(state): State<AppState>,
    _principal: Principal,
    _ws: WorkspaceContext,
) -> Result<Json<ApiResponse<GraphSchemaOverview>>, AppError> {
    let runtime = state.runtime.as_ref().ok_or_else(AppError::no_runtime)?;
    let timeout = state.timeouts.raw_query;

    let overview = tokio::time::timeout(timeout, runtime.graph_overview())
        .await
        .map_err(|_| AppError::timeout("Overview timed out".to_string()))?
        .map_err(|e| {
            error!("Graph overview failed: {e}");
            AppError::query_execution_failed(e.to_string())
        })?;

    Ok(ApiResponse::of(overview))
}

// ---------------------------------------------------------------------------
// POST /api/search/expand — get 1-hop neighbors (delegated to GraphRuntime)
// ---------------------------------------------------------------------------

use ox_ontology::graph_exploration::NodeExpansion;

fn default_expand_limit() -> usize {
    50
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct ExpandGraphRequest {
    /// Graph element ID of the node to expand.
    pub element_id: String,
    /// Max neighbors to return (default 50, capped at 200).
    #[serde(default = "default_expand_limit")]
    pub limit: usize,
}

#[utoipa::path(
    post,
    path = "/api/search/expand",
    request_body = ExpandGraphRequest,
    responses(
        (status = 200, description = "Neighbors of the node", body = NodeExpansion),
        (status = 400, description = "Missing element_id"),
        (status = 503, description = "Graph database not connected"),
        (status = 504, description = "Timeout"),
    ),
    security(("api_key" = [])),
    tag = "Query",
)]
pub(crate) async fn expand_node(
    State(state): State<AppState>,
    principal: Principal,
    _ws: WorkspaceContext,
    Json(req): Json<ExpandGraphRequest>,
) -> Result<Json<ApiResponse<NodeExpansion>>, AppError> {
    if req.element_id.trim().is_empty() {
        return Err(AppError::required_field_empty("element_id"));
    }

    let limit = req.limit.min(200);
    info!(user_id = %principal.id, element_id = %req.element_id, limit, "Expand node");

    let runtime = state.runtime.as_ref().ok_or_else(AppError::no_runtime)?;
    let timeout = state.timeouts.raw_query;

    let expansion = tokio::time::timeout(timeout, runtime.expand_node(&req.element_id, limit))
        .await
        .map_err(|_| AppError::timeout("Expand timed out".to_string()))?
        .map_err(|e| {
            error!("Expand failed: {e}");
            AppError::query_execution_failed(e.to_string())
        })?;

    Ok(ApiResponse::of(expansion))
}
