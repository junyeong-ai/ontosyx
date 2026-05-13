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

use ox_ontology::EvaluationFingerprintInput;
use ox_store::CursorParams;
use ox_store::evaluation::{
    EvaluationActual, EvaluationCase, EvaluationCaseInput, EvaluationCaseMetadata,
    EvaluationContext, EvaluationDataset, EvaluationDatasetItem, EvaluationDatasetSummary,
    EvaluationExpected, EvaluationMetric, EvaluationMetricMetadata, EvaluationRetrievedAnchor,
    EvaluationRun, EvaluationRunStatus, RetrievalComparisonOutlier, RetrievalSurface,
    RunComparisonReport, RunSummary, scope_evaluation_context,
};

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
    /// Reproducibility pins for the run — committed ontology
    /// version, dataset, model, prompt template, decoding config.
    /// Required: a run that cannot answer "which configuration
    /// produced these scores?" is uninterpretable, and the
    /// platform refuses to author one.
    pub fingerprint: EvaluationFingerprintInput,
    /// Free-form audit envelope (operator notes, run labels).
    /// Reproducibility components live on `fingerprint`.
    #[serde(default)]
    #[schema(value_type = std::collections::HashMap<String, Object>, additional_properties)]
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
    pub input: EvaluationCaseInput,
    #[serde(default)]
    pub expected: Option<EvaluationExpected>,
    #[serde(default)]
    pub actual: Option<EvaluationActual>,
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
    pub metadata: EvaluationMetricMetadata,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct EvaluationRunResponse {
    pub run: EvaluationRun,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct EvaluationCaseResponse {
    pub case: EvaluationCase,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct EvaluationMetricResponse {
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
    let fingerprint = req.fingerprint.into_fingerprint();
    let fingerprint_digest = fingerprint.digest().map_err(AppError::from)?;
    let run = EvaluationRun {
        id: Uuid::now_v7(),
        workspace_id: ws.workspace_id,
        fingerprint,
        fingerprint_digest,
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
        (status = 200, description = "Paginated run list", body = crate::openapi::EvaluationRunPage),
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
) -> Result<Json<ApiResponse<Vec<EvaluationRun>>>, AppError> {
    let cursor = CursorParams {
        cursor: params.cursor,
        limit: params.limit.unwrap_or(50),
    };
    let page = state
        .store
        .list_evaluation_runs(&cursor)
        .await
        .map_err(AppError::from)?;
    Ok(ApiResponse::page(page))
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
        metadata: EvaluationCaseMetadata::default(),
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
        (status = 200, description = "Cases for the run", body = Vec<EvaluationCase>),
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
pub(crate) async fn upsert_evaluation_metric(
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
        // Manual API-driven metric — caller-asserted score with
        // no LLM call behind it, so no provenance row to attach.
        // LLM-judged metrics flow through `judge_evaluation_case`
        // / `judge_safety_evaluation_case` which stamp the
        // provenance internally.
        provenance_id: None,
        created_at: chrono::Utc::now(),
    };
    let saved = state
        .store
        .upsert_evaluation_metric(&metric)
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
    pub input: EvaluationCaseInput,
    #[serde(default)]
    pub expected: Option<EvaluationExpected>,
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
            metadata: EvaluationCaseMetadata::default(),
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
// Adding a new kind is "extend `EvaluationCaseInput` + dispatch
// arm + brain call" — the scope wrapper, latency capture, error
// handling, and case upsert stay shared.
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ExecuteEvaluationCaseResponse {
    pub case: EvaluationCase,
}

#[utoipa::path(
    post,
    path = "/api/evaluation/runs/{run_id}/cases/{case_key}/execute",
    params(
        ("run_id" = Uuid, Path, description = "Run id"),
        ("case_key" = String, Path, description = "Stable per-run case identifier"),
    ),
    request_body = EvaluationCaseInput,
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
    Json(req): Json<EvaluationCaseInput>,
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
    // Resolve the canonical version once for any branch that needs
    // ontology-anchored data (translate-query loads the IR;
    // retrieve-anchors walks the navigation store keyed on
    // `version_id`). Only the IR materialise step is conditional —
    // navigation-only cases skip the JSONB hydrate.
    let (version_id, ir) = if req.requires_canonical_ontology() {
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
            .find_current_version(identity.id)
            .await
            .map_err(AppError::from)?
            .ok_or_else(|| {
                AppError::query_ir_invalid(
                    "workspace ontology has no committed version".to_string(),
                )
            })?;
        let ir = if matches!(
            req,
            EvaluationCaseInput::TranslateQuery { .. }
                | EvaluationCaseInput::RetrievalComparison { .. }
        ) {
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
        (Some(version.id), ir)
    } else {
        (None, None)
    };

    let input_value = req.clone();
    let expected_value = req.expected();

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
        metadata: EvaluationCaseMetadata::default(),
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
    let nav_store = Arc::clone(&state.store);
    let tokenizer_registry = Arc::clone(&state.tokenizer_registry);
    let embedder = state.memory.as_ref().map(|m| Arc::clone(m.embedder()));
    let workspace_id = ws.workspace_id;
    let started = std::time::Instant::now();
    // Outcome envelope carries the typed payload + the optional
    // `CallProvenance` from whichever LLM call produced it. The
    // case-update path stamps provenance onto `case.metadata` so
    // eval-failure drill-down resolves to the exact prompt +
    // model + render hash.
    let outcome: Result<(EvaluationActual, Option<ox_brain::CallProvenance>), String> =
        scope_evaluation_context(ctx, async move {
            match req {
                EvaluationCaseInput::TranslateQuery { question, .. } => {
                    let Some(ir) = ir else {
                        return Err(
                            "internal: ontology IR not loaded for translate_query case".to_string()
                        );
                    };
                    // Evaluation case-execute runs against the dataset's
                    // frozen ontology IR — no DomainContext / navigation
                    // store reachable here. Pass `None` so the schema RAG
                    // path on the Brain side drives the prompt context.
                    //
                    // The translate flow runs inside its own
                    // `InferenceSession` scope so attempts are
                    // recorded against the audit DAG even from the
                    // evaluation surface. The eval-case + judge
                    // metric layers stack on top: case → inference
                    // session → attempts → judge provenance.
                    let (query_ir, provenance) = ox_store::run_in_inference_session(
                        nav_store.as_ref(),
                        &question,
                        ox_ontology::AgentRef::Service {
                            service_id: "evaluation_case_execute".into(),
                        },
                        || async {
                            brain
                                .translate_query(
                                    &question,
                                    &ir,
                                    None,
                                    &entelix::ExecutionContext::default(),
                                )
                                .await
                        },
                    )
                    .await
                    .map_err(|e| e.to_string())?;
                    Ok((
                        EvaluationActual::QueryIr {
                            query_ir: Box::new(query_ir),
                        },
                        Some(provenance),
                    ))
                }
                EvaluationCaseInput::Explain { question, .. } => {
                    let output = brain
                        .explain(&question, &entelix::ExecutionContext::default())
                        .await
                        .map_err(|e| e.to_string())?;
                    Ok((
                        EvaluationActual::Explanation {
                            content: output.content,
                            model: output.model,
                        },
                        None,
                    ))
                }
                EvaluationCaseInput::RetrieveAnchors {
                    question,
                    top_k,
                    expected_anchor_ids,
                } => {
                    let Some(version_id) = version_id else {
                        return Err(
                            "internal: ontology version not resolved for retrieve_anchors case"
                                .to_string(),
                        );
                    };
                    // Cap `top_k` at 100 — the Level-3 anchor index
                    // returns at most a few hundred hits even for the
                    // broadest blend, and IR metric stability collapses
                    // past this point. Mirrors the runtime ceiling on
                    // `EntryPointSearchOptions.limit`.
                    let k = top_k.clamp(1, 100);
                    let opts = ox_store::navigation::EntryPointSearchOptions::new(
                        version_id, &question, k,
                    );
                    let hits = nav_store
                        .search_entry_points(opts)
                        .await
                        .map_err(|e| e.to_string())?;
                    // Encode the retrieved anchors as `kind:logical_id`
                    // strings — the same shape the operator authors
                    // gold-standard ids in. Round-trips through the
                    // case `actual` payload so the FE renders side-by-
                    // side without re-fetching.
                    let actual_ids: Vec<String> = hits
                        .iter()
                        .map(|h| format!("{}:{}", h.entity_kind, h.logical_id))
                        .collect();
                    let metrics = ox_store::evaluation::score_retrieval_metrics(
                        &actual_ids,
                        &expected_anchor_ids,
                        k as usize,
                    );
                    let payload = EvaluationActual::RetrievedAnchors {
                        anchor_ids: actual_ids,
                        hits: hits
                            .into_iter()
                            .map(|h| EvaluationRetrievedAnchor {
                                entity_kind: h.entity_kind,
                                logical_id: h.logical_id,
                                doc: h.doc,
                                score: h.score as f64,
                            })
                            .collect(),
                        metrics,
                    };
                    Ok((payload, None))
                }
                EvaluationCaseInput::RetrievalComparison {
                    question,
                    surface,
                    top_k,
                    expected_ids,
                } => {
                    let Some(version_id) = version_id else {
                        return Err(
                            "internal: ontology version not resolved for retrieval_comparison case"
                                .to_string(),
                        );
                    };
                    let Some(ir) = ir.as_ref() else {
                        return Err(
                            "internal: ontology IR not loaded for retrieval_comparison case"
                                .to_string(),
                        );
                    };
                    crate::routes::evaluation_retrieval::execute_retrieval_comparison(
                        crate::routes::evaluation_retrieval::ComparisonContext {
                            store: nav_store.as_ref(),
                            ir,
                            version_id,
                            workspace_id,
                            tokenizer_registry: &tokenizer_registry,
                            embedder: embedder.as_ref(),
                            question: &question,
                            surface,
                            top_k,
                            expected_ids: &expected_ids,
                        },
                    )
                    .await
                }
            }
        })
        .await;
    let elapsed_ms = started.elapsed().as_millis() as i64;

    // Stage 3 — UPSERT the case again with actual / error / latency
    // / metadata. The same natural key (`run_id`, `case_key`)
    // replaces stage 1's row in place; metrics already attached to
    // `case.id` (latency / token / cost capture) survive because
    // the case_id is preserved.
    //
    // `metadata.call.prompt_render_hash` carries the per-case
    // rendered-prompt fingerprint. Run-level pins (prompt id +
    // template version + model id + decoding config) live on the
    // `EvaluationRun.fingerprint` because they're invariant across
    // every case in the run; per-case Call metadata only carries
    // the render hash because that's the dimension that varies
    // case to case.
    let (actual, error_msg, provenance) = match outcome {
        Ok((value, prov)) => (Some(value), None, prov),
        Err(msg) => (None, Some(msg), None),
    };
    let metadata = match provenance {
        Some(prov) => EvaluationCaseMetadata::Call {
            prompt_render_hash: prov.prompt_render_hash,
        },
        None => EvaluationCaseMetadata::default(),
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
        metadata,
        created_at: case.created_at,
    };
    let case = state
        .store
        .upsert_evaluation_case(&updated)
        .await
        .map_err(AppError::from)?;

    // Stage 4 — for retrieval cases, lift the deterministic IR
    // metrics off the `actual` payload and persist them as
    // `evaluation_metrics` rows. The dashboard / diff surfaces
    // pick them up identically to LLM-judged axes; the case is
    // "complete" the moment execute returns (no judge round-trip
    // needed). Skip silently when the brain call failed —
    // `case.actual` will be None and the FE renders the error
    // path instead.
    if let Some(EvaluationActual::RetrievedAnchors { metrics, .. }) = case.actual.as_ref() {
        let axes: [(&str, f64); 4] = [
            ("retrieval.precision_at_k", metrics.precision_at_k),
            ("retrieval.recall_at_k", metrics.recall_at_k),
            ("retrieval.mrr", metrics.mrr),
            ("retrieval.ndcg_at_k", metrics.ndcg_at_k),
        ];
        let metric_metadata = EvaluationMetricMetadata::Retrieval {
            k: metrics.k,
            topk_hit_count: metrics.topk_hit_count,
            expected_count: metrics.expected_count,
        };
        for (name, score) in axes {
            let row = EvaluationMetric {
                id: Uuid::now_v7(),
                case_id: case.id,
                workspace_id: ws.workspace_id,
                name: name.to_string(),
                score,
                reasoning: None,
                metadata: metric_metadata.clone(),
                // Deterministic IR metric (no LLM call) — the
                // case-level provenance carries the activity that
                // produced the underlying retrieval anchors.
                provenance_id: None,
                created_at: chrono::Utc::now(),
            };
            state
                .store
                .upsert_evaluation_metric(&row)
                .await
                .map_err(AppError::from)?;
        }
    }

    // Hybrid-vs-baseline comparison: persist 8 metric rows
    // (`<surface>.<leg>.<axis>`) so the dashboard's standard
    // axis chart pivots both legs identically. FE computes
    // `lift = hybrid - trigram` per axis on display — keeping
    // lift derived rather than persisted means future
    // re-runs that flip one leg's score don't drift the
    // other axis's lift out of date.
    if let Some(EvaluationActual::RetrievalComparison {
        surface,
        hybrid,
        trigram,
    }) = case.actual.as_ref()
    {
        let surface_tag = surface.as_str();
        for (leg_tag, leg) in [("hybrid", hybrid), ("trigram", trigram)] {
            let metric_metadata = EvaluationMetricMetadata::Retrieval {
                k: leg.metrics.k,
                topk_hit_count: leg.metrics.topk_hit_count,
                expected_count: leg.metrics.expected_count,
            };
            let axes: [(&str, f64); 4] = [
                ("precision_at_k", leg.metrics.precision_at_k),
                ("recall_at_k", leg.metrics.recall_at_k),
                ("mrr", leg.metrics.mrr),
                ("ndcg_at_k", leg.metrics.ndcg_at_k),
            ];
            for (axis, score) in axes {
                let name = format!("{surface_tag}.{leg_tag}.{axis}");
                let row = EvaluationMetric {
                    id: Uuid::now_v7(),
                    case_id: case.id,
                    workspace_id: ws.workspace_id,
                    name,
                    score,
                    reasoning: None,
                    metadata: metric_metadata.clone(),
                    provenance_id: None,
                    created_at: chrono::Utc::now(),
                };
                state
                    .store
                    .upsert_evaluation_metric(&row)
                    .await
                    .map_err(AppError::from)?;
            }
        }
    }

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

    // The case input envelope is already the typed
    // `EvaluationCaseInput`. Pull the question off whichever
    // variant landed; the dispatch matches the execute side so
    // adding a new operation extends both arms together.
    // Retrieval cases land their metrics deterministically at
    // execute time — there's nothing for the LLM judge to score
    // (the IR axes don't benefit from a rubric). Reject early
    // rather than silently re-judging on top of the deterministic
    // axes.
    if matches!(&case.input, EvaluationCaseInput::RetrieveAnchors { .. }) {
        return Err(AppError::query_ir_invalid(
            "retrieve_anchors cases score deterministically at execute \
             time and are not LLM-judgeable"
                .to_string(),
        ));
    }
    let question = case.input.question().to_string();
    let expected_json = case.expected.as_ref().map(AppError::to_json).transpose()?;
    let actual_json = AppError::to_json(actual)?;

    let (judgement, prov) = state
        .brain
        .judge_evaluation_case(
            &question,
            expected_json.as_ref(),
            &actual_json,
            &entelix::ExecutionContext::default(),
        )
        .await
        .map_err(AppError::from)?;

    let provenance_id = record_judge_provenance(&state, &case, &prov).await?;

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
            metadata: EvaluationMetricMetadata::Judge {
                run_id: case.run_id,
                case_key: case.case_key.clone(),
                source: None,
            },
            provenance_id: Some(provenance_id),
            created_at: now,
        };
        let saved = state
            .store
            .upsert_evaluation_metric(&metric)
            .await
            .map_err(AppError::from)?;
        metrics.push(saved);
    }

    Ok(ApiResponse::of(JudgeEvaluationCaseResponse { metrics }))
}

/// Stamp a PROV-O activity row for a judge invocation against
/// `case`, returning the `provenance_id` that the judge's metric
/// rows attach to. Subject is the synthetic
/// `evaluation_case:<case_id>` label (the audit row attaches to
/// the case, not to any one metric — the case is what the judge
/// scored). Plan + agent come straight from the LLM's
/// `CallProvenance`.
async fn record_judge_provenance(
    state: &AppState,
    case: &EvaluationCase,
    prov: &ox_brain::CallProvenance,
) -> Result<Uuid, AppError> {
    let plan = ox_ontology::ProvenancePlan {
        template_id: prov.prompt_id.clone(),
        template_version: prov.prompt_version.clone(),
        prompt_render_hash: prov.prompt_render_hash.clone(),
    };
    let capture = ox_ontology::ProvenanceCapture::draft_proposal(plan, prov.model_id.clone())
        .with_used(std::iter::once(ox_ontology::EntityRef::Arbitrary {
            label: format!("evaluation_run:{}", case.run_id),
        }));
    let id_str = state
        .store
        .record_activity(
            capture,
            ox_ontology::EntityRef::Arbitrary {
                label: format!("evaluation_case:{}", case.id),
            },
        )
        .await
        .map_err(AppError::from)?;
    Uuid::parse_str(id_str.as_str()).map_err(|e| {
        AppError::internal(format!(
            "ProvenanceStore::record_activity returned non-UUID id `{}`: {e}",
            id_str.as_str()
        ))
    })
}

// ---------------------------------------------------------------------------
// Handler — judge a case along the safety axes
//
// Distinct from the RAGAS judge: scores the answer along
// `safety.toxicity_safe`, `safety.pii_safe`,
// `safety.factual_correctness`, `safety.harmfulness_safe`.
// `1.0 = safest` for every axis so the dashboard's "higher is
// better" colouring works without a per-axis flip. Independent
// from the RAGAS axes — a case can carry both rubrics (the
// `safety.*` prefix prevents UPSERT collisions on
// `(case_id, name)`).
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/api/evaluation/cases/{case_id}/judge_safety",
    params(("case_id" = Uuid, Path, description = "Case id")),
    responses(
        (status = 200, description = "Safety judgement recorded", body = JudgeEvaluationCaseResponse),
        (status = 400, description = "Case has no `actual` to judge",
            body = inline(crate::openapi::ErrorResponse)),
        (status = 404, description = "Case not found",
            body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Evaluation",
)]
#[tracing::instrument(skip(state, principal))]
pub(crate) async fn judge_safety_evaluation_case(
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

    // Retrieval cases skip the safety judge for the same reason
    // they skip the RAGAS judge — there's no LLM-produced answer
    // to score, just deterministic IR axes that landed at execute
    // time.
    if matches!(&case.input, EvaluationCaseInput::RetrieveAnchors { .. }) {
        return Err(AppError::query_ir_invalid(
            "retrieve_anchors cases score deterministically at execute time \
             and are not LLM-judgeable on the safety axes either"
                .to_string(),
        ));
    }
    let question = case.input.question().to_string();
    let actual_json = AppError::to_json(actual)?;

    let (judgement, prov) = state
        .brain
        .judge_safety_evaluation_case(
            &question,
            &actual_json,
            &entelix::ExecutionContext::default(),
        )
        .await
        .map_err(AppError::from)?;

    let provenance_id = record_judge_provenance(&state, &case, &prov).await?;

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
            metadata: EvaluationMetricMetadata::SafetyJudge {
                run_id: case.run_id,
                case_key: case.case_key.clone(),
                source: None,
            },
            provenance_id: Some(provenance_id),
            created_at: now,
        };
        let saved = state
            .store
            .upsert_evaluation_metric(&metric)
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
        (status = 200, description = "Metrics for the case", body = Vec<EvaluationMetric>),
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

// ---------------------------------------------------------------------------
// Datasets — frozen `(input, expected)` pairs reusable across runs
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct UpsertEvaluationDatasetRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct UpsertEvaluationDatasetItemEntry {
    pub item_key: String,
    pub input: EvaluationCaseInput,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected: Option<EvaluationExpected>,
    #[serde(default)]
    #[schema(value_type = std::collections::HashMap<String, Object>, additional_properties)]
    pub metadata: serde_json::Value,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct ReplaceEvaluationDatasetItemsRequest {
    pub items: Vec<UpsertEvaluationDatasetItemEntry>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct EvaluationDatasetResponse {
    pub dataset: EvaluationDataset,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ReplaceEvaluationDatasetItemsResponse {
    pub dataset_id: Uuid,
    pub item_count: u64,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateRunFromDatasetRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Reproducibility pins. `fingerprint.dataset_id` names the
    /// dataset whose items are materialised into cases.
    pub fingerprint: EvaluationFingerprintInput,
    #[serde(default)]
    #[schema(value_type = std::collections::HashMap<String, Object>, additional_properties)]
    pub metadata: serde_json::Value,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct CreateRunFromDatasetResponse {
    pub run: EvaluationRun,
    /// Number of dataset items materialised into cases. Allows
    /// the FE to surface "12 cases ready to execute" without a
    /// follow-up `list_evaluation_cases` call.
    pub case_count: u64,
}

/// `POST /api/evaluation/datasets` — upsert a dataset by
/// `(workspace_id, name)`. Re-importing under the same name
/// preserves `id` + `created_at` and updates `description` only;
/// every downstream FK (runs that referenced this id, items
/// keyed on this id) survives the re-import.
#[utoipa::path(
    post,
    path = "/api/evaluation/datasets",
    request_body = UpsertEvaluationDatasetRequest,
    responses(
        (status = 200, description = "Dataset upserted", body = EvaluationDatasetResponse),
    ),
    security(("api_key" = [])),
    tag = "Evaluation",
)]
pub(crate) async fn upsert_evaluation_dataset(
    State(state): State<AppState>,
    principal: Principal,
    ws: WorkspaceContext,
    Json(req): Json<UpsertEvaluationDatasetRequest>,
) -> Result<Json<ApiResponse<EvaluationDatasetResponse>>, AppError> {
    principal.require_admin()?;
    let dataset = EvaluationDataset {
        id: Uuid::now_v7(),
        workspace_id: ws.workspace_id,
        name: req.name,
        description: req.description,
        created_at: chrono::Utc::now(),
    };
    let saved = state
        .store
        .upsert_evaluation_dataset(&dataset)
        .await
        .map_err(AppError::from)?;
    Ok(ApiResponse::of(EvaluationDatasetResponse {
        dataset: saved,
    }))
}

// ---------------------------------------------------------------------------
// Handler — promote a chat-sample case to a curated dataset
//
// The online sampler (`eval_sampler`) drops chat completions
// into `live_chat_samples` cases. Operators triage those, and
// when a case looks like a useful regression anchor they want
// to promote it into a named dataset for repeatable runs.
// Without this endpoint, the promotion path is a manual
// copy/paste through the dataset-import surface — slow + lossy.
//
// The promotion is a pure case → dataset-item upsert: pulls the
// case's `input` (and any captured `actual` as the expected
// reference, if the operator opts in), generates a stable
// `item_key`, lands it through `upsert_evaluation_dataset_item`.
// Re-promoting the same case is idempotent because the
// natural-key UPSERT collapses on the generated key.
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct PromoteCaseToDatasetRequest {
    pub dataset_id: Uuid,
    /// When true, the case's captured `actual` payload becomes
    /// the dataset item's `expected`. Useful when operator
    /// already manually verified the answer is correct and wants
    /// to pin it as the regression target. Default false — the
    /// item lands `expected`-less, leaving the gold answer to
    /// be authored separately.
    #[serde(default)]
    pub use_actual_as_expected: bool,
    /// Optional override for the generated `item_key`. Default
    /// (`None`) generates `sample-{case.case_key}` so the
    /// promoted item traces back to its origin.
    #[serde(default)]
    pub item_key: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct PromoteCaseToDatasetResponse {
    pub item: ox_store::evaluation::EvaluationDatasetItem,
}

#[utoipa::path(
    post,
    path = "/api/evaluation/cases/{case_id}/promote-to-dataset",
    params(("case_id" = Uuid, Path, description = "Case id")),
    request_body = PromoteCaseToDatasetRequest,
    responses(
        (status = 200, description = "Case promoted to dataset item", body = PromoteCaseToDatasetResponse),
        (status = 404, description = "Case or dataset not found",
            body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Evaluation",
)]
#[tracing::instrument(skip(state, principal, req))]
pub(crate) async fn promote_case_to_dataset(
    State(state): State<AppState>,
    principal: Principal,
    ws: WorkspaceContext,
    Path(case_id): Path<Uuid>,
    Json(req): Json<PromoteCaseToDatasetRequest>,
) -> Result<Json<ApiResponse<PromoteCaseToDatasetResponse>>, AppError> {
    principal.require_admin()?;

    let case = state
        .store
        .get_evaluation_case(case_id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("EvaluationCase"))?;

    // Resolve the dataset to ensure it exists in the workspace.
    // RLS would block a cross-workspace dataset lookup anyway,
    // but surfacing a clear 404 beats a silent insert into a
    // ghost dataset.
    let _dataset = state
        .store
        .get_evaluation_dataset(req.dataset_id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("EvaluationDataset"))?;

    let item_key = req
        .item_key
        .clone()
        .unwrap_or_else(|| format!("sample-{}", case.case_key));
    let expected = if req.use_actual_as_expected {
        case.actual.as_ref().map(EvaluationExpected::from_actual)
    } else {
        None
    };
    let item = ox_store::evaluation::EvaluationDatasetItem {
        id: Uuid::now_v7(),
        dataset_id: req.dataset_id,
        workspace_id: ws.workspace_id,
        item_key,
        input: case.input.clone(),
        expected,
        metadata: serde_json::json!({
            "promoted_from_case_id": case.id,
            "promoted_from_run_id": case.run_id,
            "promoted_from_case_key": case.case_key,
        }),
        created_at: chrono::Utc::now(),
    };
    let saved = state
        .store
        .upsert_evaluation_dataset_item(&item)
        .await
        .map_err(AppError::from)?;
    Ok(ApiResponse::of(PromoteCaseToDatasetResponse {
        item: saved,
    }))
}

/// `GET /api/evaluation/datasets` — list datasets in the active
/// workspace, newest-created first.
#[utoipa::path(
    get,
    path = "/api/evaluation/datasets",
    params(
        ("limit" = Option<u32>, Query, description = "Page size (default 50, max 100)"),
        ("cursor" = Option<String>, Query, description = "Opaque cursor"),
    ),
    responses(
        (status = 200, description = "Dataset page", body = crate::openapi::EvaluationDatasetSummaryPage),
    ),
    security(("api_key" = [])),
    tag = "Evaluation",
)]
pub(crate) async fn list_evaluation_datasets(
    State(state): State<AppState>,
    _principal: Principal,
    _ws: WorkspaceContext,
    Query(pagination): Query<CursorParams>,
) -> Result<Json<ApiResponse<Vec<EvaluationDatasetSummary>>>, AppError> {
    let page = state
        .store
        .list_evaluation_datasets(&pagination)
        .await
        .map_err(AppError::from)?;
    Ok(ApiResponse::page(page))
}

/// `GET /api/evaluation/datasets/{id}` — fetch a single dataset
/// header. Items live behind the sibling
/// `/api/evaluation/datasets/{id}/items` endpoint to keep this
/// path lightweight when the FE only needs the header.
#[utoipa::path(
    get,
    path = "/api/evaluation/datasets/{id}",
    params(("id" = Uuid, Path)),
    responses(
        (status = 200, description = "Dataset", body = EvaluationDatasetResponse),
        (status = 404, description = "Dataset not found", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Evaluation",
)]
pub(crate) async fn get_evaluation_dataset(
    State(state): State<AppState>,
    _principal: Principal,
    _ws: WorkspaceContext,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<EvaluationDatasetResponse>>, AppError> {
    let dataset = state
        .store
        .get_evaluation_dataset(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("EvaluationDataset"))?;
    Ok(ApiResponse::of(EvaluationDatasetResponse { dataset }))
}

/// `DELETE /api/evaluation/datasets/{id}` — cascade-delete the
/// dataset + items. Runs that referenced the dataset stay alive
/// (`evaluation_runs.dataset_id` `SET NULL`).
#[utoipa::path(
    delete,
    path = "/api/evaluation/datasets/{id}",
    params(("id" = Uuid, Path)),
    responses(
        (status = 204, description = "Deleted"),
        (status = 404, description = "Dataset not found", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Evaluation",
)]
pub(crate) async fn delete_evaluation_dataset(
    State(state): State<AppState>,
    principal: Principal,
    _ws: WorkspaceContext,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    principal.require_admin()?;
    let deleted = state
        .store
        .delete_evaluation_dataset(id)
        .await
        .map_err(AppError::from)?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::not_found("EvaluationDataset"))
    }
}

/// `GET /api/evaluation/datasets/{id}/items` — list every frozen
/// `(input, expected)` pair in the dataset, ordered by
/// `item_key`.
#[utoipa::path(
    get,
    path = "/api/evaluation/datasets/{id}/items",
    params(("id" = Uuid, Path)),
    responses(
        (status = 200, description = "Items", body = Vec<EvaluationDatasetItem>),
    ),
    security(("api_key" = [])),
    tag = "Evaluation",
)]
pub(crate) async fn list_evaluation_dataset_items(
    State(state): State<AppState>,
    _principal: Principal,
    _ws: WorkspaceContext,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<Vec<EvaluationDatasetItem>>>, AppError> {
    let items = state
        .store
        .list_evaluation_dataset_items(id)
        .await
        .map_err(AppError::from)?;
    Ok(ApiResponse::of(items))
}

/// `PUT /api/evaluation/datasets/{id}/items` — replace every
/// item under `dataset_id` with the supplied set in one
/// transaction. Items not in the request body are deleted; items
/// in both DB + request body are upserted on `(dataset_id,
/// item_key)`. Atomic — partial import never lands.
#[utoipa::path(
    put,
    path = "/api/evaluation/datasets/{id}/items",
    params(("id" = Uuid, Path)),
    request_body = ReplaceEvaluationDatasetItemsRequest,
    responses(
        (status = 200, description = "Items replaced", body = ReplaceEvaluationDatasetItemsResponse),
    ),
    security(("api_key" = [])),
    tag = "Evaluation",
)]
pub(crate) async fn replace_evaluation_dataset_items(
    State(state): State<AppState>,
    principal: Principal,
    ws: WorkspaceContext,
    Path(id): Path<Uuid>,
    Json(req): Json<ReplaceEvaluationDatasetItemsRequest>,
) -> Result<Json<ApiResponse<ReplaceEvaluationDatasetItemsResponse>>, AppError> {
    principal.require_admin()?;
    let now = chrono::Utc::now();
    let items: Vec<EvaluationDatasetItem> = req
        .items
        .into_iter()
        .map(|entry| EvaluationDatasetItem {
            id: Uuid::now_v7(),
            dataset_id: id,
            workspace_id: ws.workspace_id,
            item_key: entry.item_key,
            input: entry.input,
            expected: entry.expected,
            metadata: entry.metadata,
            created_at: now,
        })
        .collect();
    let count = state
        .store
        .replace_evaluation_dataset_items(id, &items)
        .await
        .map_err(AppError::from)?;
    Ok(ApiResponse::of(ReplaceEvaluationDatasetItemsResponse {
        dataset_id: id,
        item_count: count,
    }))
}

/// `POST /api/evaluation/runs/from-dataset` — materialise a fresh
/// run from a dataset. Atomic dataset-read + run-insert + bulk
/// case-insert in one transaction. Returns the created run plus
/// the case count for the FE response panel.
#[utoipa::path(
    post,
    path = "/api/evaluation/runs/from-dataset",
    request_body = CreateRunFromDatasetRequest,
    responses(
        (status = 201, description = "Run created", body = CreateRunFromDatasetResponse),
        (status = 404, description = "Dataset not found", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Evaluation",
)]
pub(crate) async fn create_run_from_dataset(
    State(state): State<AppState>,
    principal: Principal,
    _ws: WorkspaceContext,
    Json(req): Json<CreateRunFromDatasetRequest>,
) -> Result<(StatusCode, Json<ApiResponse<CreateRunFromDatasetResponse>>), AppError> {
    principal.require_admin()?;
    let fingerprint = req.fingerprint.into_fingerprint();
    let (run, case_count) = state
        .store
        .create_run_from_dataset(&req.name, &req.description, fingerprint, req.metadata)
        .await
        .map_err(AppError::from)?;
    Ok((
        StatusCode::CREATED,
        ApiResponse::of(CreateRunFromDatasetResponse { run, case_count }),
    ))
}

// ---------------------------------------------------------------------------
// Run summary — case counts + per-axis aggregate in one round trip
// ---------------------------------------------------------------------------

/// `GET /api/evaluation/runs/{run_id}/summary` — case counts +
/// per-axis aggregate. Drives the run-list "judged 5/12 ·
/// faithfulness 0.78" badge so operators triage without
/// drilling into each detail page. RAGAS axes (`faithfulness`
/// / `answer_relevance` / …) and safety axes
/// (`safety.toxicity_safe` / …) ride together; the
/// `axis_means[]` ordering is alphabetic for stable rendering.
#[utoipa::path(
    get,
    path = "/api/evaluation/runs/{run_id}/summary",
    params(("run_id" = Uuid, Path, description = "Run id")),
    responses(
        (status = 200, description = "Run summary", body = RunSummary),
    ),
    security(("api_key" = [])),
    tag = "Evaluation",
)]
pub(crate) async fn evaluation_run_summary(
    State(state): State<AppState>,
    _principal: Principal,
    _ws: WorkspaceContext,
    Path(run_id): Path<Uuid>,
) -> Result<Json<ApiResponse<RunSummary>>, AppError> {
    let summary = state
        .store
        .evaluation_run_summary(run_id)
        .await
        .map_err(AppError::from)?;
    Ok(ApiResponse::of(summary))
}

// ---------------------------------------------------------------------------
// Comparison outlier drill-down — worst-case lifts per (surface, axis)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct ComparisonOutliersQuery {
    /// Optional surface filter — `verified_query` /
    /// `community_summary` / `knowledge_entry`. Absent →
    /// every surface.
    #[serde(default)]
    pub surface: Option<RetrievalSurface>,
    /// Optional axis filter — `precision_at_k` / `recall_at_k`
    /// / `mrr` / `ndcg_at_k`. Absent → every axis.
    #[serde(default)]
    pub axis: Option<String>,
    /// Maximum rows returned. Server caps at 100; default 10.
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ComparisonOutliersResponse {
    pub outliers: Vec<RetrievalComparisonOutlier>,
}

/// `GET /api/evaluation/runs/{run_id}/comparison-outliers` —
/// worst-first list of case-level retrieval-comparison outliers.
/// Drives the dashboard's per-cell drill-down: the operator
/// clicks a (surface, axis) cell whose mean lift is low, and
/// this endpoint surfaces the bad-actor cases dragging the
/// average down.
#[utoipa::path(
    get,
    path = "/api/evaluation/runs/{run_id}/comparison-outliers",
    params(
        ("run_id" = Uuid, Path, description = "Run id"),
        ComparisonOutliersQuery,
    ),
    responses(
        (status = 200, description = "Worst-first comparison outliers", body = ComparisonOutliersResponse),
    ),
    security(("api_key" = [])),
    tag = "Evaluation",
)]
pub(crate) async fn list_run_comparison_outliers(
    State(state): State<AppState>,
    _principal: Principal,
    _ws: WorkspaceContext,
    Path(run_id): Path<Uuid>,
    Query(req): Query<ComparisonOutliersQuery>,
) -> Result<Json<ApiResponse<ComparisonOutliersResponse>>, AppError> {
    let outliers = state
        .store
        .list_run_comparison_outliers(
            run_id,
            req.surface,
            req.axis.as_deref(),
            req.limit.unwrap_or(10),
        )
        .await
        .map_err(AppError::from)?;
    Ok(ApiResponse::of(ComparisonOutliersResponse { outliers }))
}

// ---------------------------------------------------------------------------
// Run comparison — Phoenix/Braintrust regression diff
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CompareRunsQuery {
    pub baseline: Uuid,
    pub candidate: Uuid,
}

/// `GET /api/evaluation/runs/diff?baseline=<id>&candidate=<id>` —
/// per-axis delta + per-case row diff between two runs that
/// materialised from the same dataset. Returns a typed
/// `validation_error` 400 when the runs don't share a dataset
/// (the case_key correspondence the diff requires).
#[utoipa::path(
    get,
    path = "/api/evaluation/runs/diff",
    params(
        ("baseline" = Uuid, Query, description = "Baseline run id"),
        ("candidate" = Uuid, Query, description = "Candidate run id"),
    ),
    responses(
        (status = 200, description = "Diff", body = RunComparisonReport),
        (status = 400, description = "Runs don't share a dataset",
            body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Evaluation",
)]
pub(crate) async fn compare_evaluation_runs(
    State(state): State<AppState>,
    _principal: Principal,
    ws: WorkspaceContext,
    Query(q): Query<CompareRunsQuery>,
) -> Result<Json<ApiResponse<RunComparisonReport>>, AppError> {
    // Workspace-customised regression policy (threshold +
    // min-N) overrides the platform default. Missing /
    // malformed settings fall back transparently — the store
    // method handles the absence path.
    let policy = state
        .store
        .get_evaluation_settings(ws.workspace_id)
        .await
        .map_err(AppError::from)?
        .regression_policy();
    let report = state
        .store
        .compare_evaluation_runs(q.baseline, q.candidate, policy)
        .await
        .map_err(AppError::from)?;

    // Fan out the regression alerts to every channel subscribed
    // to `retrieval_lift_regression`. Fire-and-forget so the
    // diff response stays snappy regardless of webhook latency;
    // failures land on `notification_logs.status = "failed"`
    // for operator audit. `spawn_scoped` captures
    // `WORKSPACE_ID` so the inner store calls (channel list,
    // log insert) succeed under RLS.
    if !report.retrieval_lift_regressions.is_empty() {
        let store_clone = state.store.clone();
        let ws_id = ws.workspace_id;
        let baseline = q.baseline;
        let candidate = q.candidate;
        let alerts = report.retrieval_lift_regressions.clone();
        ox_context::spawn_scoped(async move {
            crate::notifications::dispatch_retrieval_lift_regression(
                store_clone.as_ref(),
                ws_id,
                baseline,
                candidate,
                &alerts,
            )
            .await;
        });
    }

    Ok(ApiResponse::of(report))
}

// ---------------------------------------------------------------------------
// Workspace evaluation settings — regression alarm threshold + min-N
// ---------------------------------------------------------------------------

/// `GET /api/evaluation/settings` — read this workspace's
/// evaluation settings. Missing settings resolve to platform
/// defaults; the FE renders the same form whether the workspace
/// has overridden or not.
#[utoipa::path(
    get,
    path = "/api/evaluation/settings",
    responses(
        (status = 200, description = "Workspace evaluation settings",
            body = ox_store::evaluation::WorkspaceEvaluationSettings),
    ),
    security(("api_key" = [])),
    tag = "Evaluation",
)]
pub(crate) async fn get_evaluation_settings(
    State(state): State<AppState>,
    _principal: Principal,
    ws: WorkspaceContext,
) -> Result<Json<ApiResponse<ox_store::evaluation::WorkspaceEvaluationSettings>>, AppError> {
    let settings = state
        .store
        .get_evaluation_settings(ws.workspace_id)
        .await
        .map_err(AppError::from)?;
    Ok(ApiResponse::of(settings))
}

/// `PUT /api/evaluation/settings` — admin-gated update of this
/// workspace's evaluation settings. Validation runs at the
/// route boundary so an invalid threshold never lands in the
/// JSONB. Other settings keys (locale chains, future
/// namespaces) round-trip unchanged.
#[utoipa::path(
    put,
    path = "/api/evaluation/settings",
    request_body = ox_store::evaluation::WorkspaceEvaluationSettings,
    responses(
        (status = 200, description = "Updated", body = ox_store::evaluation::WorkspaceEvaluationSettings),
        (status = 400, description = "Validation failure",
            body = inline(crate::openapi::ErrorResponse)),
        (status = 403, description = "Admin role required",
            body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Evaluation",
)]
pub(crate) async fn update_evaluation_settings(
    State(state): State<AppState>,
    principal: Principal,
    ws: WorkspaceContext,
    Json(req): Json<ox_store::evaluation::WorkspaceEvaluationSettings>,
) -> Result<Json<ApiResponse<ox_store::evaluation::WorkspaceEvaluationSettings>>, AppError> {
    principal.require_admin()?;
    if let Err(message) = req.validate() {
        return Err(AppError::validation("evaluation_settings", message));
    }
    state
        .store
        .update_evaluation_settings(ws.workspace_id, &req)
        .await
        .map_err(AppError::from)?;
    Ok(ApiResponse::of(req))
}
