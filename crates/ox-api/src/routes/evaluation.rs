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

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use ox_store::evaluation::{
    scope_evaluation_context, EvaluationCase, EvaluationContext, EvaluationMetric,
    EvaluationRun, EvaluationRunStatus,
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

// ---------------------------------------------------------------------------
// Handler — bulk case upsert
//
// Operator-facing dataset seed surface. Accepts an array of
// (case_key, input, expected?) tuples and runs a sequential
// `upsert_evaluation_case` per row. The natural-key UPSERT
// `(run_id, case_key)` makes the call idempotent — re-importing
// the same dataset replaces in place, dataset edits land
// without a separate diff endpoint, and partial-success errors
// surface as a typed list rather than aborting the whole batch.
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct BulkUpsertEvaluationCaseEntry {
    pub case_key: String,
    #[schema(value_type = Object)]
    pub input: serde_json::Value,
    #[serde(default)]
    #[schema(value_type = Option<Object>)]
    pub expected: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct BulkUpsertEvaluationCasesRequest {
    pub cases: Vec<BulkUpsertEvaluationCaseEntry>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct BulkUpsertEvaluationCaseError {
    pub case_key: String,
    pub message: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct BulkUpsertEvaluationCasesResponse {
    pub upserted_count: usize,
    /// Per-row errors. Empty when every row succeeded; a
    /// non-empty list means partial success — the caller can
    /// retry just the failed `case_key`s.
    pub errors: Vec<BulkUpsertEvaluationCaseError>,
}

#[utoipa::path(
    post,
    path = "/api/evaluation/runs/{run_id}/cases/bulk",
    params(("run_id" = Uuid, Path, description = "Run id")),
    request_body = BulkUpsertEvaluationCasesRequest,
    responses(
        (status = 200, description = "Bulk upsert outcome — partial success returns errors[]",
            body = BulkUpsertEvaluationCasesResponse),
    ),
    security(("api_key" = [])),
    tag = "Evaluation",
)]
#[tracing::instrument(skip(state, principal, req))]
pub(crate) async fn bulk_upsert_evaluation_cases(
    State(state): State<AppState>,
    principal: Principal,
    ws: WorkspaceContext,
    Path(run_id): Path<Uuid>,
    Json(req): Json<BulkUpsertEvaluationCasesRequest>,
) -> Result<Json<ApiResponse<BulkUpsertEvaluationCasesResponse>>, AppError> {
    principal.require_admin()?;

    let mut upserted_count = 0usize;
    let mut errors = Vec::new();
    let now = chrono::Utc::now();
    for entry in req.cases {
        let case = EvaluationCase {
            id: Uuid::now_v7(),
            run_id,
            workspace_id: ws.workspace_id,
            case_key: entry.case_key.clone(),
            input: entry.input,
            expected: entry.expected,
            actual: None,
            error: None,
            latency_ms: None,
            created_at: now,
        };
        match state.store.upsert_evaluation_case(&case).await {
            Ok(_) => {
                upserted_count += 1;
            }
            Err(err) => {
                errors.push(BulkUpsertEvaluationCaseError {
                    case_key: entry.case_key,
                    message: err.to_string(),
                });
            }
        }
    }

    Ok(ApiResponse::of(BulkUpsertEvaluationCasesResponse {
        upserted_count,
        errors,
    }))
}

// ---------------------------------------------------------------------------
// Handler — case execute
//
// The endpoint that closes the RAGAS loop. Operator hands a case
// (input + golden expected) and the BE runs the active brain
// operation under an `EvaluationContext` scope, capturing
// `latency_ms.<operation>` automatically and persisting the
// `actual` output / error onto the case row.
//
// Today only `translate_query` is wired. Adding a new kind is
// "extend the request enum + dispatch arm + brain call" — the
// scope wrapper, latency capture, error handling, and case
// upsert stay shared.
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExecuteEvaluationCaseRequest {
    /// Translate a natural-language question into `QueryIR` against
    /// the workspace's canonical ontology. Requires the workspace
    /// to have a committed canonical version.
    TranslateQuery {
        question: String,
        /// Golden / reference `QueryIR` for downstream judge
        /// comparison. Stored on `evaluation_cases.expected` so
        /// the dataset survives re-runs.
        #[serde(default)]
        #[schema(value_type = Option<Object>)]
        expected_query_ir: Option<serde_json::Value>,
    },
    /// Free-form natural-language explanation. Does not require
    /// a canonical ontology — useful for evaluating chat-style
    /// answer quality independent of the schema.
    Explain {
        question: String,
        /// Optional reference answer for downstream comparison.
        #[serde(default)]
        expected_answer: Option<String>,
    },
}

impl ExecuteEvaluationCaseRequest {
    /// True when the dispatch needs the workspace's canonical
    /// ontology + IR loaded. Drives the up-front load in stage 1
    /// of the handler — operations that don't need an ontology
    /// (chat explain, generic LLM probes) skip the load and run
    /// during the greenfield phase.
    fn requires_canonical_ontology(&self) -> bool {
        matches!(self, Self::TranslateQuery { .. })
    }

    fn question(&self) -> &str {
        match self {
            Self::TranslateQuery { question, .. } => question,
            Self::Explain { question, .. } => question,
        }
    }

    fn expected_value(&self) -> Option<serde_json::Value> {
        match self {
            Self::TranslateQuery {
                expected_query_ir, ..
            } => expected_query_ir.clone(),
            Self::Explain {
                expected_answer, ..
            } => expected_answer.clone().map(serde_json::Value::String),
        }
    }
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ExecuteEvaluationCaseResponse {
    #[schema(value_type = Object)]
    pub case: EvaluationCase,
}

#[utoipa::path(
    post,
    path = "/api/evaluation/runs/{run_id}/cases/{case_key}/execute",
    params(
        ("run_id" = Uuid, Path, description = "Run id"),
        ("case_key" = String, Path, description = "Stable per-run case identifier"),
    ),
    request_body = ExecuteEvaluationCaseRequest,
    responses(
        (status = 200, description = "Case executed and persisted", body = ExecuteEvaluationCaseResponse),
        (status = 400, description = "Workspace has no canonical ontology yet",
            body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Evaluation",
)]
#[tracing::instrument(skip(state, principal, req))]
pub(crate) async fn execute_evaluation_case(
    State(state): State<AppState>,
    principal: Principal,
    ws: WorkspaceContext,
    Path((run_id, case_key)): Path<(Uuid, String)>,
    Json(req): Json<ExecuteEvaluationCaseRequest>,
) -> Result<Json<ApiResponse<ExecuteEvaluationCaseResponse>>, AppError> {
    principal.require_admin()?;

    // Resolve the workspace ontology + IR up front for operations
    // that need it so an error here surfaces before the case row
    // is touched. Operations that don't (chat explain, generic
    // LLM probes) skip the load entirely. The case input is
    // persisted regardless — the operator should see their input
    // on the case page even if the brain call later fails (the
    // resulting `error` field on the case row carries the failure
    // reason for triage).
    let ir = if req.requires_canonical_ontology() {
        let identity = state
            .store
            .get_workspace_ontology()
            .await
            .map_err(AppError::from)?
            .ok_or_else(|| {
                AppError::query_ir_invalid(
                    "workspace has no canonical ontology yet — commit a draft \
                     before executing translate_query cases"
                        .to_string(),
                )
            })?;
        let version = state
            .store
            .get_current_version(identity.id)
            .await
            .map_err(AppError::from)?
            .ok_or_else(|| {
                AppError::query_ir_invalid(
                    "workspace ontology has no committed version".to_string(),
                )
            })?;
        Some(
            state
                .store
                .get_ontology_ir(version.id)
                .await
                .map_err(AppError::from)?
                .ok_or_else(|| {
                    AppError::query_ir_invalid(
                        "workspace ontology snapshot is unavailable".to_string(),
                    )
                })?,
        )
    } else {
        None
    };

    let input_value = serde_json::to_value(&req).map_err(|e| {
        AppError::query_ir_invalid(format!("failed to serialize case input: {e}"))
    })?;
    let expected_value = req.expected_value();

    // Stage 1 — UPSERT the case (input + expected). `actual` /
    // `latency_ms` / `error` are populated in stage 3 after the
    // brain call resolves.
    let initial = EvaluationCase {
        id: Uuid::now_v7(),
        run_id,
        workspace_id: ws.workspace_id,
        case_key: case_key.clone(),
        input: input_value.clone(),
        expected: expected_value.clone(),
        actual: None,
        error: None,
        latency_ms: None,
        created_at: chrono::Utc::now(),
    };
    let case = state
        .store
        .upsert_evaluation_case(&initial)
        .await
        .map_err(AppError::from)?;

    // Stage 2 — execute under the evaluation scope. Brain's
    // `call_structured_traced` reads the task-local + capture
    // hook and emits `latency_ms.translate_query` for free.
    let ctx = EvaluationContext {
        run_id,
        case_key: case_key.clone(),
        case_id: case.id,
    };
    let brain = Arc::clone(&state.brain);
    let started = std::time::Instant::now();
    let outcome: Result<serde_json::Value, String> = scope_evaluation_context(ctx, async move {
        match req {
            ExecuteEvaluationCaseRequest::TranslateQuery { question, .. } => {
                let Some(ir) = ir else {
                    return Err(
                        "internal: ontology IR not loaded for translate_query case"
                            .to_string(),
                    );
                };
                let query_ir = brain
                    .translate_query(
                        &question,
                        &ir,
                        &branchforge::ExecutionContext::empty(),
                    )
                    .await
                    .map_err(|e| e.to_string())?;
                serde_json::to_value(&query_ir).map_err(|e| {
                    format!("failed to serialize translate_query output: {e}")
                })
            }
            ExecuteEvaluationCaseRequest::Explain { question, .. } => {
                let output = brain.explain(&question).await.map_err(|e| e.to_string())?;
                Ok(serde_json::json!({
                    "content": output.content,
                    "model": output.model,
                }))
            }
        }
    })
    .await;
    let elapsed_ms = started.elapsed().as_millis() as i64;

    // Stage 3 — UPSERT the case again with actual / error / latency.
    // The same natural key (`run_id`, `case_key`) replaces stage 1's
    // row in place; metrics already attached to `case.id` (latency
    // capture) survive because the case_id is preserved.
    let (actual, error_msg) = match outcome {
        Ok(value) => (Some(value), None),
        Err(msg) => (None, Some(msg)),
    };
    let updated = EvaluationCase {
        id: case.id,
        run_id,
        workspace_id: ws.workspace_id,
        case_key: case_key.clone(),
        input: input_value,
        expected: expected_value,
        actual,
        error: error_msg,
        latency_ms: Some(elapsed_ms),
        created_at: case.created_at,
    };
    let case = state
        .store
        .upsert_evaluation_case(&updated)
        .await
        .map_err(AppError::from)?;

    Ok(ApiResponse::of(ExecuteEvaluationCaseResponse { case }))
}

// ---------------------------------------------------------------------------
// Handler — judge a case
//
// Reads the case's input + expected + actual, calls the LLM
// judge, and records each axis as a fresh `evaluation_metrics`
// row. The natural-key UPSERT on `(case_id, name)` means
// re-judging replaces the previous score in place; latency
// metrics from the case-execute path stay attached.
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct JudgeEvaluationCaseResponse {
    /// One row per recorded axis. Returned in canonical RAGAS
    /// order (faithfulness → answer_relevance → context_precision
    /// → context_recall) so the FE can render a stable column
    /// ordering without re-sorting.
    #[schema(value_type = Vec<Object>)]
    pub metrics: Vec<EvaluationMetric>,
}

#[utoipa::path(
    post,
    path = "/api/evaluation/cases/{case_id}/judge",
    params(("case_id" = Uuid, Path, description = "Case id")),
    responses(
        (status = 200, description = "Judgement recorded", body = JudgeEvaluationCaseResponse),
        (status = 400, description = "Case has no `actual` to judge",
            body = inline(crate::openapi::ErrorResponse)),
        (status = 404, description = "Case not found",
            body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Evaluation",
)]
#[tracing::instrument(skip(state, principal))]
pub(crate) async fn judge_evaluation_case(
    State(state): State<AppState>,
    principal: Principal,
    ws: WorkspaceContext,
    Path(case_id): Path<Uuid>,
) -> Result<Json<ApiResponse<JudgeEvaluationCaseResponse>>, AppError> {
    principal.require_admin()?;

    let case = state
        .store
        .get_evaluation_case(case_id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("EvaluationCase"))?;

    let actual = case.actual.as_ref().ok_or_else(|| {
        AppError::query_ir_invalid(
            "case has no `actual` to judge — execute the case first".to_string(),
        )
    })?;

    // The case input envelope is the discriminated
    // `ExecuteEvaluationCaseRequest`. Pull the question off
    // whichever variant landed; today only `translate_query` is
    // judgeable, but the dispatch matches the execute side so
    // adding a new operation extends both arms together.
    let parsed: ExecuteEvaluationCaseRequest = serde_json::from_value(case.input.clone())
        .map_err(|e| {
            AppError::query_ir_invalid(format!(
                "case input does not match a known executable shape: {e}"
            ))
        })?;
    let question = parsed.question().to_string();

    let judgement = state
        .brain
        .judge_evaluation_case(&question, case.expected.as_ref(), actual)
        .await
        .map_err(AppError::from)?;

    let mut metrics = Vec::with_capacity(4);
    let now = chrono::Utc::now();
    for (name, score, reasoning) in judgement.axes() {
        let metric = EvaluationMetric {
            id: Uuid::now_v7(),
            case_id: case.id,
            workspace_id: ws.workspace_id,
            name: name.to_string(),
            score,
            reasoning: Some(reasoning.to_string()),
            metadata: serde_json::json!({
                "kind": "judge",
                "run_id": case.run_id,
                "case_key": case.case_key,
            }),
            created_at: now,
        };
        let saved = state
            .store
            .record_evaluation_metric(&metric)
            .await
            .map_err(AppError::from)?;
        metrics.push(saved);
    }

    Ok(ApiResponse::of(JudgeEvaluationCaseResponse { metrics }))
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
