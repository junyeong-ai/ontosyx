use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::{Deserialize, Serialize};
use tracing::{error, info};
use uuid::Uuid;

use ox_core::query_ir::{QueryIR, QueryResult};
use ox_core::types::PropertyValue;
use ox_store::{CursorPage, CursorParams, QueryExecution, QueryExecutionSummary};

use crate::error::AppError;
use crate::principal::Principal;
use crate::state::AppState;
use crate::workspace::WorkspaceContext;

// ---------------------------------------------------------------------------
// POST /api/search — full-text search across Neo4j graph nodes
// ---------------------------------------------------------------------------

fn default_search_limit() -> usize {
    20
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct GraphSearchRequest {
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
    request_body = GraphSearchRequest,
    responses(
        (status = 200, description = "Search results", body = Object),
        (status = 400, description = "Empty query", body = inline(crate::openapi::ErrorResponse)),
        (status = 503, description = "Graph database not connected", body = inline(crate::openapi::ErrorResponse)),
        (status = 504, description = "Query timeout", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Query",
)]
pub async fn search_graph(
    State(state): State<AppState>,
    principal: Principal,
    ws: WorkspaceContext,
    Json(req): Json<GraphSearchRequest>,
) -> Result<Json<Vec<ox_core::graph_exploration::SearchResultNode>>, AppError> {
    let search_term = req.query.trim().to_string();
    if search_term.is_empty() {
        return Err(AppError::bad_request("query must not be empty"));
    }

    let limit = req.limit.min(100);
    info!(user_id = %principal.id, query = %search_term, limit, "Graph search");

    let runtime = state.runtime.as_ref().ok_or_else(AppError::no_runtime)?;
    let timeout = state.timeouts.raw_query;
    let labels = req.labels.as_deref();

    let mut results =
        tokio::time::timeout(timeout, runtime.search_nodes(&search_term, limit, labels))
            .await
            .map_err(|_| {
                AppError::timeout(format!("Search timed out after {}s", timeout.as_secs()))
            })?
            .map_err(|e| {
                error!("Graph search failed: {e}");
                AppError::unprocessable(format!("Search execution failed: {e}"))
            })?;

    // Apply ACL enforcement
    if let Ok(policies) = state
        .store
        .get_effective_policies(
            principal.role.as_str(),
            ws.workspace_role.as_str(),
            principal.user_uuid().ok(),
        )
        .await
    {
        crate::acl_enforcement::apply_acl_to_search_results(&mut results, &policies);
    }

    Ok(Json(results))
}

// ---------------------------------------------------------------------------
// POST /api/query/raw — direct query execution (power users)
//
// Accepts a raw query in the target language (e.g., Cypher).
// Skips NL translation — zero LLM calls.
// ---------------------------------------------------------------------------

#[derive(Deserialize, utoipa::ToSchema)]
pub struct QueryRawRequest {
    /// Raw query statement in the target language (e.g., Cypher).
    pub query: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct QueryRawResponse {
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
    request_body = QueryRawRequest,
    responses(
        (status = 200, description = "Raw query result", body = QueryRawResponse),
        (status = 400, description = "Empty query", body = inline(crate::openapi::ErrorResponse)),
        (status = 422, description = "Query execution failed", body = inline(crate::openapi::ErrorResponse)),
        (status = 503, description = "Graph database not connected", body = inline(crate::openapi::ErrorResponse)),
        (status = 504, description = "Query timeout", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Query",
)]
#[tracing::instrument(skip(state, principal, req))]
pub async fn raw_query(
    State(state): State<AppState>,
    principal: Principal,
    ws: WorkspaceContext,
    Json(req): Json<QueryRawRequest>,
) -> Result<Json<QueryRawResponse>, AppError> {
    if req.query.trim().is_empty() {
        return Err(AppError::bad_request("query must not be empty"));
    }

    // Block write operations unless user has designer role
    let upper = req.query.to_uppercase();
    const WRITE_KEYWORDS: &[&str] = &["DELETE", "DETACH", "CREATE", "MERGE", "SET ", "REMOVE "];
    let has_write = WRITE_KEYWORDS.iter().any(|kw| upper.contains(kw));
    if has_write {
        principal.require_designer()?;
    }

    let target = state.compiler.target_name().to_string();
    info!(user_id = %principal.id, target = %target, "Raw query submitted");

    let runtime = state.runtime.as_ref().ok_or_else(AppError::no_runtime)?;

    let timeout = state.timeouts.raw_query;
    let empty_params: HashMap<String, PropertyValue> = HashMap::new();
    let start = std::time::Instant::now();
    let results = tokio::time::timeout(timeout, runtime.execute_query(&req.query, &empty_params))
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
            AppError::unprocessable(format!("Query execution failed: {e}"))
        })?;
    crate::metrics::record_query("ok", start.elapsed());

    // Apply ACL enforcement
    let mut results = results;
    if let Ok(policies) = state
        .store
        .get_effective_policies(
            principal.role.as_str(),
            ws.workspace_role.as_str(),
            principal.user_uuid().ok(),
        )
        .await
    {
        crate::acl_enforcement::apply_acl_policies(&mut results, &policies);
    }

    // Record metering (fire-and-forget)
    let execution_time_ms = start.elapsed().as_millis() as i64;
    let row_count = results.metadata.rows_returned;
    {
        let meter_store = Arc::clone(&state.store);
        let meter_user = principal.user_uuid().ok();
        crate::spawn_scoped::spawn_scoped(async move {
            let _ = meter_store
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
                .await;
        });
    }

    Ok(Json(QueryRawResponse {
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
pub async fn list_executions(
    State(state): State<AppState>,
    principal: Principal,
    Query(params): Query<CursorParams>,
) -> Result<Json<CursorPage<QueryExecutionSummary>>, AppError> {
    let page = state
        .store
        .list_query_executions(&principal.id, &params)
        .await?;
    Ok(Json(page))
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
pub async fn get_execution(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
) -> Result<Json<QueryExecution>, AppError> {
    let execution = state
        .store
        .get_query_execution(&principal.id, id)
        .await?
        .ok_or_else(AppError::execution_not_found)?;
    Ok(Json(execution))
}

// ---------------------------------------------------------------------------
// PATCH /api/query/history/:id/feedback — submit accuracy feedback
// ---------------------------------------------------------------------------

#[derive(Deserialize, utoipa::ToSchema)]
pub struct QueryFeedbackRequest {
    /// "positive", "negative", or null to clear feedback
    pub feedback: Option<String>,
}

const VALID_FEEDBACK: &[&str] = &["positive", "negative"];

#[utoipa::path(
    patch,
    path = "/api/query/history/{id}/feedback",
    params(("id" = Uuid, Path, description = "Query execution ID")),
    request_body = QueryFeedbackRequest,
    responses(
        (status = 200, description = "Feedback recorded"),
        (status = 400, description = "Invalid feedback value"),
        (status = 404, description = "Execution not found"),
    ),
    security(("bearer" = [])),
    tag = "Query",
)]
pub async fn set_feedback(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
    Json(req): Json<QueryFeedbackRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    if let Some(ref fb) = req.feedback
        && !VALID_FEEDBACK.contains(&fb.as_str())
    {
        return Err(AppError::bad_request(format!(
            "Invalid feedback '{fb}'. Valid values: positive, negative, or null to clear"
        )));
    }

    let updated = state
        .store
        .update_query_feedback(&principal.id, id, req.feedback.as_deref())
        .await
        .map_err(AppError::from)?;

    if !updated {
        return Err(AppError::execution_not_found());
    }

    Ok(Json(serde_json::json!({ "status": "ok" })))
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
    /// Optional ontology ID for context (used during deserialization).
    #[allow(dead_code)]
    pub ontology_id: Option<String>,
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
pub async fn execute_from_ir(
    State(state): State<AppState>,
    principal: Principal,
    ws: WorkspaceContext,
    Json(req): Json<ExecuteFromIrRequest>,
) -> Result<Json<ExecuteFromIrResponse>, AppError> {
    let target = state.compiler.target_name().to_string();
    info!(user_id = %principal.id, target = %target, "QueryIR execution submitted");

    // Step 1: Compile QueryIR → target language
    let compiled = state.compiler.compile_query(&req.query_ir).map_err(|e| {
        error!("QueryIR compilation failed: {e}");
        AppError::unprocessable(format!("QueryIR compilation failed: {e}"))
    })?;

    // Step 2: Execute the compiled query
    let runtime = state.runtime.as_ref().ok_or_else(AppError::no_runtime)?;

    let timeout = state.timeouts.raw_query;
    let start = std::time::Instant::now();
    let results = tokio::time::timeout(
        timeout,
        runtime.execute_query(&compiled.statement, &compiled.params),
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
        AppError::unprocessable(format!("Query execution failed: {e}"))
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

    // Apply ACL enforcement
    let mut results = results;
    if let Ok(policies) = state
        .store
        .get_effective_policies(
            principal.role.as_str(),
            ws.workspace_role.as_str(),
            principal.user_uuid().ok(),
        )
        .await
    {
        crate::acl_enforcement::apply_acl_policies(&mut results, &policies);
    }

    // Record metering (fire-and-forget)
    let execution_time_ms = start.elapsed().as_millis() as i64;
    let row_count = results.metadata.rows_returned;
    {
        let meter_store = Arc::clone(&state.store);
        let meter_user = principal.user_uuid().ok();
        crate::spawn_scoped::spawn_scoped(async move {
            let _ = meter_store
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
                .await;
        });
    }

    Ok(Json(ExecuteFromIrResponse {
        compiled_query: compiled.statement,
        compiled_target: target,
        result: results,
        widget_hint,
    }))
}

// ---------------------------------------------------------------------------
// GET /api/graph/overview — graph schema overview (delegated to GraphRuntime)
// ---------------------------------------------------------------------------

use ox_core::graph_exploration::GraphSchemaOverview;

#[utoipa::path(
    get,
    path = "/api/graph/overview",
    responses(
        (status = 200, description = "Graph schema overview", body = Object),
        (status = 503, description = "Graph database not connected"),
        (status = 504, description = "Timeout"),
    ),
    security(("api_key" = [])),
    tag = "Query",
)]
pub async fn graph_overview(
    State(state): State<AppState>,
    _principal: Principal,
    _ws: WorkspaceContext,
) -> Result<Json<GraphSchemaOverview>, AppError> {
    let runtime = state.runtime.as_ref().ok_or_else(AppError::no_runtime)?;
    let timeout = state.timeouts.raw_query;

    let overview = tokio::time::timeout(timeout, runtime.graph_overview())
        .await
        .map_err(|_| AppError::timeout("Overview timed out".to_string()))?
        .map_err(|e| {
            error!("Graph overview failed: {e}");
            AppError::unprocessable(format!("Overview failed: {e}"))
        })?;

    Ok(Json(overview))
}

// ---------------------------------------------------------------------------
// POST /api/search/expand — get 1-hop neighbors (delegated to GraphRuntime)
// ---------------------------------------------------------------------------

use ox_core::graph_exploration::NodeExpansion;

fn default_expand_limit() -> usize {
    50
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct GraphExpandRequest {
    /// Graph element ID of the node to expand.
    pub element_id: String,
    /// Max neighbors to return (default 50, capped at 200).
    #[serde(default = "default_expand_limit")]
    pub limit: usize,
}

#[utoipa::path(
    post,
    path = "/api/search/expand",
    request_body = GraphExpandRequest,
    responses(
        (status = 200, description = "Neighbors of the node", body = Object),
        (status = 400, description = "Missing element_id"),
        (status = 503, description = "Graph database not connected"),
        (status = 504, description = "Timeout"),
    ),
    security(("api_key" = [])),
    tag = "Query",
)]
pub async fn expand_node(
    State(state): State<AppState>,
    principal: Principal,
    ws: WorkspaceContext,
    Json(req): Json<GraphExpandRequest>,
) -> Result<Json<NodeExpansion>, AppError> {
    if req.element_id.trim().is_empty() {
        return Err(AppError::bad_request("element_id must not be empty"));
    }

    let limit = req.limit.min(200);
    info!(user_id = %principal.id, element_id = %req.element_id, limit, "Expand node");

    let runtime = state.runtime.as_ref().ok_or_else(AppError::no_runtime)?;
    let timeout = state.timeouts.raw_query;

    let mut expansion = tokio::time::timeout(timeout, runtime.expand_node(&req.element_id, limit))
        .await
        .map_err(|_| AppError::timeout("Expand timed out".to_string()))?
        .map_err(|e| {
            error!("Expand failed: {e}");
            AppError::unprocessable(format!("Expand failed: {e}"))
        })?;

    // Apply ACL enforcement
    if let Ok(policies) = state
        .store
        .get_effective_policies(
            principal.role.as_str(),
            ws.workspace_role.as_str(),
            principal.user_uuid().ok(),
        )
        .await
    {
        crate::acl_enforcement::apply_acl_to_node_expansion(&mut expansion, &policies);
    }

    Ok(Json(expansion))
}
