//! `/api/evaluation` — RAGAS-style metric loop surface.
//!
//! Five resources, all scoped to the active workspace and admin-
//! gated by default (operators measure model quality; tenants do
//! not author their own runs):
//!
//! - `runs`       — batch metadata (one row per evaluation campaign).
//! - `cases`      — per-run input / expected / actual tuples.
//! - `metrics`    — per-case axis scores (faithfulness, relevance, …).
//!
//! The persistence contract lives in `ox-store::evaluation` and
//! `EvaluationStore`; this module is the HTTP envelope. Storage
//! UPSERT semantics mean the FE can drive the same dataset
//! against multiple judges without orphaning earlier rows — the
//! latest score on `(case_id, name)` wins.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use ox_store::evaluation::{
    EvaluationCase, EvaluationMetric, EvaluationRun, EvaluationRunStatus,
};
use ox_store::{CursorPage, CursorParams};

use crate::error::AppError;
use crate::principal::Principal;
use crate::response::ApiResponse;
use crate::state::AppState;
use crate::workspace::WorkspaceContext;

// ---------------------------------------------------------------------------
// Request / response DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateEvaluationRunRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Optional pin to a committed ontology version. Absent for
    /// pre-canonical / draft-stage evaluations.
    #[serde(default)]
    pub ontology_version_id: Option<Uuid>,
    /// Run-level configuration envelope. See
    /// `ox_store::evaluation::EvaluationRun.metadata`.
    #[serde(default)]
    #[schema(value_type = Object)]
    pub metadata: serde_json::Value,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CompleteEvaluationRunRequest {
    /// Terminal state — `succeeded`, `failed`, or `cancelled`. The
    /// handler rejects `running` because a complete-call must
    /// always pin a finished status. Wire shape is the snake_case
    /// string (matches the storage enum's wire form).
    pub status: String,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct UpsertEvaluationCaseRequest {
    /// Stable per-run identifier. Required so the natural-key
    /// UPSERT replaces the same case on re-runs.
    pub case_key: String,
    #[schema(value_type = Object)]
    pub input: serde_json::Value,
    #[serde(default)]
    #[schema(value_type = Option<Object>)]
    pub expected: Option<serde_json::Value>,
    #[serde(default)]
    #[schema(value_type = Option<Object>)]
    pub actual: Option<serde_json::Value>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub latency_ms: Option<i64>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct RecordEvaluationMetricRequest {
    /// Rubric axis name. RAGAS canonicals
    /// (`faithfulness` / `answer_relevance` / `context_precision`
    /// / `context_recall`) plus tenant-defined names ride on the
    /// same column.
    pub name: String,
    /// Score on the rubric axis. Conventionally `[0.0, 1.0]` but
    /// the column is unbounded so a latency-style rubric (ms) can
    /// reuse the same surface.
    pub score: f64,
    #[serde(default)]
    pub reasoning: Option<String>,
    #[serde(default)]
    #[schema(value_type = Object)]
    pub metadata: serde_json::Value,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct EvaluationRunResponse {
    #[schema(value_type = Object)]
    pub run: EvaluationRun,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct EvaluationCaseResponse {
    #[schema(value_type = Object)]
    pub case: EvaluationCase,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct EvaluationMetricResponse {
    #[schema(value_type = Object)]
    pub metric: EvaluationMetric,
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct ListEvaluationRunsQuery {
    pub cursor: Option<String>,
    pub limit: Option<u32>,
}

// ---------------------------------------------------------------------------
// Handlers — runs
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/api/evaluation/runs",
    request_body = CreateEvaluationRunRequest,
    responses(
        (status = 201, description = "Created run", body = EvaluationRunResponse),
        (status = 403, description = "Admin role required",
            body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Evaluation",
)]
#[tracing::instrument(skip(state, principal, req))]
pub(crate) async fn create_evaluation_run(
    State(state): State<AppState>,
    principal: Principal,
    ws: WorkspaceContext,
    Json(req): Json<CreateEvaluationRunRequest>,
) -> Result<(StatusCode, Json<ApiResponse<EvaluationRunResponse>>), AppError> {
    principal.require_admin()?;
    let run = EvaluationRun {
        id: Uuid::now_v7(),
        workspace_id: ws.workspace_id,
        ontology_version_id: req.ontology_version_id,
        name: req.name,
        description: req.description,
        status: EvaluationRunStatus::Running,
        started_at: chrono::Utc::now(),
        completed_at: None,
        metadata: req.metadata,
    };
    let saved = state
        .store
        .create_evaluation_run(&run)
        .await
        .map_err(AppError::from)?;
    Ok((
        StatusCode::CREATED,
        ApiResponse::of(EvaluationRunResponse { run: saved }),
    ))
}

#[utoipa::path(
    get,
    path = "/api/evaluation/runs/{id}",
    params(("id" = Uuid, Path, description = "Run id")),
    responses(
        (status = 200, description = "Run detail", body = EvaluationRunResponse),
        (status = 404, description = "Not found",
            body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Evaluation",
)]
#[tracing::instrument(skip(state, _principal))]
pub(crate) async fn get_evaluation_run(
    State(state): State<AppState>,
    _principal: Principal,
    _ws: WorkspaceContext,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<EvaluationRunResponse>>, AppError> {
    let run = state
        .store
        .get_evaluation_run(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("EvaluationRun"))?;
    Ok(ApiResponse::of(EvaluationRunResponse { run }))
}

#[utoipa::path(
    get,
    path = "/api/evaluation/runs",
    params(ListEvaluationRunsQuery),
    responses(
        (status = 200, description = "Paginated run list", body = Object),
    ),
    security(("api_key" = [])),
    tag = "Evaluation",
)]
#[tracing::instrument(skip(state, _principal))]
pub(crate) async fn list_evaluation_runs(
    State(state): State<AppState>,
    _principal: Principal,
    _ws: WorkspaceContext,
    Query(params): Query<ListEvaluationRunsQuery>,
) -> Result<Json<ApiResponse<CursorPage<EvaluationRun>>>, AppError> {
    let cursor = CursorParams {
        cursor: params.cursor,
        limit: params.limit.unwrap_or(50),
    };
    let page = state
        .store
        .list_evaluation_runs(&cursor)
        .await
        .map_err(AppError::from)?;
    Ok(ApiResponse::of(page))
}

#[utoipa::path(
    post,
    path = "/api/evaluation/runs/{id}/complete",
    params(("id" = Uuid, Path, description = "Run id")),
    request_body = CompleteEvaluationRunRequest,
    responses(
        (status = 200, description = "Updated run", body = EvaluationRunResponse),
        (status = 400, description = "Status must be terminal",
            body = inline(crate::openapi::ErrorResponse)),
        (status = 404, description = "Not found",
            body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Evaluation",
)]
#[tracing::instrument(skip(state, principal, req))]
pub(crate) async fn complete_evaluation_run(
    State(state): State<AppState>,
    principal: Principal,
    _ws: WorkspaceContext,
    Path(id): Path<Uuid>,
    Json(req): Json<CompleteEvaluationRunRequest>,
) -> Result<Json<ApiResponse<EvaluationRunResponse>>, AppError> {
    principal.require_admin()?;
    let status = EvaluationRunStatus::from_wire_str(&req.status).ok_or_else(|| {
        AppError::query_ir_invalid(format!(
            "unknown evaluation run status `{}`; expected one of `succeeded`, `failed`, `cancelled`",
            req.status
        ))
    })?;
    if !status.is_terminal() {
        return Err(AppError::query_ir_invalid(
            "complete_evaluation_run requires a terminal status (succeeded/failed/cancelled)"
                .to_string(),
        ));
    }
    let run = state
        .store
        .complete_evaluation_run(id, status)
        .await
        .map_err(AppError::from)?;
    Ok(ApiResponse::of(EvaluationRunResponse { run }))
}

#[utoipa::path(
    delete,
    path = "/api/evaluation/runs/{id}",
    params(("id" = Uuid, Path, description = "Run id")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 404, description = "Not found",
            body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Evaluation",
)]
#[tracing::instrument(skip(state, principal))]
pub(crate) async fn delete_evaluation_run(
    State(state): State<AppState>,
    principal: Principal,
    _ws: WorkspaceContext,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    principal.require_admin()?;
    let removed = state
        .store
        .delete_evaluation_run(id)
        .await
        .map_err(AppError::from)?;
    if !removed {
        return Err(AppError::not_found("EvaluationRun"));
    }
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Handlers — cases
// ---------------------------------------------------------------------------

#[utoipa::path(
    put,
    path = "/api/evaluation/runs/{run_id}/cases",
    params(("run_id" = Uuid, Path, description = "Run id")),
    request_body = UpsertEvaluationCaseRequest,
    responses(
        (status = 200, description = "Upserted case", body = EvaluationCaseResponse),
    ),
    security(("api_key" = [])),
    tag = "Evaluation",
)]
#[tracing::instrument(skip(state, principal, req))]
pub(crate) async fn upsert_evaluation_case(
    State(state): State<AppState>,
    principal: Principal,
    ws: WorkspaceContext,
    Path(run_id): Path<Uuid>,
    Json(req): Json<UpsertEvaluationCaseRequest>,
) -> Result<Json<ApiResponse<EvaluationCaseResponse>>, AppError> {
    principal.require_admin()?;
    let case = EvaluationCase {
        id: Uuid::now_v7(),
        run_id,
        workspace_id: ws.workspace_id,
        case_key: req.case_key,
        input: req.input,
        expected: req.expected,
        actual: req.actual,
        error: req.error,
        latency_ms: req.latency_ms,
        created_at: chrono::Utc::now(),
    };
    let saved = state
        .store
        .upsert_evaluation_case(&case)
        .await
        .map_err(AppError::from)?;
    Ok(ApiResponse::of(EvaluationCaseResponse { case: saved }))
}

#[utoipa::path(
    get,
    path = "/api/evaluation/runs/{run_id}/cases",
    params(("run_id" = Uuid, Path, description = "Run id")),
    responses(
        (status = 200, description = "Cases for the run", body = Object),
    ),
    security(("api_key" = [])),
    tag = "Evaluation",
)]
#[tracing::instrument(skip(state, _principal))]
pub(crate) async fn list_evaluation_cases(
    State(state): State<AppState>,
    _principal: Principal,
    _ws: WorkspaceContext,
    Path(run_id): Path<Uuid>,
) -> Result<Json<ApiResponse<Vec<EvaluationCase>>>, AppError> {
    let cases = state
        .store
        .list_evaluation_cases(run_id)
        .await
        .map_err(AppError::from)?;
    Ok(ApiResponse::of(cases))
}

// ---------------------------------------------------------------------------
// Handlers — metrics
// ---------------------------------------------------------------------------

#[utoipa::path(
    put,
    path = "/api/evaluation/cases/{case_id}/metrics",
    params(("case_id" = Uuid, Path, description = "Case id")),
    request_body = RecordEvaluationMetricRequest,
    responses(
        (status = 200, description = "Recorded metric", body = EvaluationMetricResponse),
    ),
    security(("api_key" = [])),
    tag = "Evaluation",
)]
#[tracing::instrument(skip(state, principal, req))]
pub(crate) async fn record_evaluation_metric(
    State(state): State<AppState>,
    principal: Principal,
    ws: WorkspaceContext,
    Path(case_id): Path<Uuid>,
    Json(req): Json<RecordEvaluationMetricRequest>,
) -> Result<Json<ApiResponse<EvaluationMetricResponse>>, AppError> {
    principal.require_admin()?;
    let metric = EvaluationMetric {
        id: Uuid::now_v7(),
        case_id,
        workspace_id: ws.workspace_id,
        name: req.name,
        score: req.score,
        reasoning: req.reasoning,
        metadata: req.metadata,
        created_at: chrono::Utc::now(),
    };
    let saved = state
        .store
        .record_evaluation_metric(&metric)
        .await
        .map_err(AppError::from)?;
    Ok(ApiResponse::of(EvaluationMetricResponse { metric: saved }))
}

#[utoipa::path(
    get,
    path = "/api/evaluation/cases/{case_id}/metrics",
    params(("case_id" = Uuid, Path, description = "Case id")),
    responses(
        (status = 200, description = "Metrics for the case", body = Object),
    ),
    security(("api_key" = [])),
    tag = "Evaluation",
)]
#[tracing::instrument(skip(state, _principal))]
pub(crate) async fn list_evaluation_metrics(
    State(state): State<AppState>,
    _principal: Principal,
    _ws: WorkspaceContext,
    Path(case_id): Path<Uuid>,
) -> Result<Json<ApiResponse<Vec<EvaluationMetric>>>, AppError> {
    let metrics = state
        .store
        .list_evaluation_metrics(case_id)
        .await
        .map_err(AppError::from)?;
    Ok(ApiResponse::of(metrics))
}
