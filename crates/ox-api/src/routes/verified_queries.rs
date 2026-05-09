//! `/api/verified-queries` — operator surface for the verified
//! Q→IR bank (Φ11.4).
//!
//! Five resources:
//!
//! - `POST   /api/verified-queries`                    — promote.
//! - `GET    /api/verified-queries`                    — list with optional status filter.
//! - `GET    /api/verified-queries/{id}`               — detail.
//! - `POST   /api/verified-queries/{id}/transition-status` — lifecycle transition.
//! - `DELETE /api/verified-queries/{id}`               — hard delete.
//!
//! All admin-gated by `principal.require_designer()` — verified
//! queries become ICL exemplars at runtime, so promotion + status
//! authority lives with workspace designers, not end users.
//!
//! ## Promotion default = `UnderReview`
//!
//! `POST /api/verified-queries` defaults the row's status to
//! `UnderReview` even when a designer promotes — the explicit
//! review handoff (admin queue → `Verified`) is the canonical
//! audit path. Designers that want immediate retrievability pass
//! `status = "verified"` explicitly. The freshness cron (Φ11.3)
//! later flips `Verified → Stale` on schema drift; admin
//! re-runs the transition to bring it back.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use chrono::Utc;
use serde::{Deserialize, Serialize};

use ox_ontology::{
    AgentRef, ComplexityClass, VerifiedQueryDef, VerifiedQueryId, VerifiedQueryStatus,
    question_hash,
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
pub struct PromoteVerifiedQueryRequest {
    /// Free-text question — the natural-language wording the
    /// operator validated. Persisted verbatim; the canonical
    /// `question_hash` is computed server-side from
    /// `canonicalize_question(question)`.
    pub question: String,
    /// `QueryIR` JSON envelope. Server does not re-validate the
    /// IR's semantic correctness — that's the designer's
    /// responsibility at promotion time. The freshness cron
    /// later flags rows whose IR references unknown labels.
    #[schema(value_type = std::collections::HashMap<String, Object>, additional_properties)]
    pub query_ir: serde_json::Value,
    pub complexity_class: ComplexityClass,
    /// Initial lifecycle state. Defaults to `UnderReview` so a
    /// chat-side `SaveAsVqr` always lands in the review queue;
    /// admin paths that want immediate retrievability pass
    /// `verified` explicitly.
    #[serde(default)]
    pub status: Option<VerifiedQueryStatus>,
    #[serde(default)]
    pub description: String,
    /// Optional explicit id. Absent → server generates
    /// `vq-{question_hash}` (deterministic per workspace +
    /// canonical question).
    #[serde(default)]
    pub id: Option<String>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct TransitionVerifiedQueryStatusRequest {
    pub status: VerifiedQueryStatus,
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct ListVerifiedQueriesQuery {
    /// Filter to a single status (e.g. `verified` for the
    /// retrievable bank, `under_review` for the admin queue).
    /// Absent → return every status.
    pub status: Option<VerifiedQueryStatus>,
    /// Page size. Server caps at 1000 (matches store layer
    /// enforcement); absent → 100.
    pub limit: Option<u32>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct VerifiedQueryListResponse {
    pub rows: Vec<VerifiedQueryDef>,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/api/verified-queries",
    request_body = PromoteVerifiedQueryRequest,
    responses(
        (status = 201, description = "Verified query promoted", body = VerifiedQueryDef),
        (status = 403, description = "Designer role required",
            body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "VerifiedQueries",
)]
#[tracing::instrument(skip(state, principal, req))]
pub(crate) async fn promote_verified_query(
    State(state): State<AppState>,
    principal: Principal,
    ws: WorkspaceContext,
    Json(req): Json<PromoteVerifiedQueryRequest>,
) -> Result<(StatusCode, Json<ApiResponse<VerifiedQueryDef>>), AppError> {
    principal.require_designer()?;

    if req.question.trim().is_empty() {
        return Err(AppError::required_field_empty("question"));
    }
    if !req.query_ir.is_object() {
        return Err(AppError::validation(
            "query_ir",
            "query_ir must be a JSON object representing the typed QueryIR shape",
        ));
    }

    let q_hash = question_hash(&req.question);
    let id = req
        .id
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| format!("vq-{q_hash}"));
    let status = req.status.unwrap_or(VerifiedQueryStatus::UnderReview);
    let now = Utc::now();

    // Φ11.5 — embed the canonical question if the brain has an
    // embedder attached, so the row enters the bank with a
    // semantic-NN-ready vector. Cold-start deployments (no
    // embedder) write `embedding = None`; the trigram retriever
    // continues to surface the row without it.
    let embedding = embed_question_for_verified_query(&state, &req.question).await;

    let tokens = crate::tokenizer_publish::tokenize_for_workspace(
        &state,
        ws.workspace_id,
        &req.question,
    )
    .await;

    let vq = VerifiedQueryDef {
        id: VerifiedQueryId::new(id),
        workspace_id: ws.workspace_id,
        question: req.question,
        question_hash: q_hash,
        query_ir: req.query_ir,
        complexity_class: req.complexity_class,
        status,
        author: AgentRef::User {
            user_id: principal.id.clone(),
        },
        description: req.description,
        verified_at: now,
        updated_at: now,
        embedding,
        tokenized_text: tokens.tokenized_text,
        tokenizer_dict_fingerprint: tokens.tokenizer_dict_fingerprint,
    };
    let saved = state
        .store
        .upsert_verified_query(&vq)
        .await
        .map_err(AppError::from)?;
    Ok((StatusCode::CREATED, ApiResponse::of(saved)))
}

#[utoipa::path(
    get,
    path = "/api/verified-queries",
    params(ListVerifiedQueriesQuery),
    responses(
        (status = 200, description = "Verified query list", body = VerifiedQueryListResponse),
    ),
    security(("api_key" = [])),
    tag = "VerifiedQueries",
)]
#[tracing::instrument(skip(state, _principal))]
pub(crate) async fn list_verified_queries(
    State(state): State<AppState>,
    _principal: Principal,
    _ws: WorkspaceContext,
    Query(q): Query<ListVerifiedQueriesQuery>,
) -> Result<Json<ApiResponse<VerifiedQueryListResponse>>, AppError> {
    let limit = q.limit.unwrap_or(100);
    let rows = state
        .store
        .list_verified_queries(q.status, limit)
        .await
        .map_err(AppError::from)?;
    Ok(ApiResponse::of(VerifiedQueryListResponse { rows }))
}

#[utoipa::path(
    get,
    path = "/api/verified-queries/{id}",
    params(("id" = String, Path, description = "Verified query id")),
    responses(
        (status = 200, description = "Verified query detail", body = VerifiedQueryDef),
        (status = 404, description = "Not found",
            body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "VerifiedQueries",
)]
#[tracing::instrument(skip(state, _principal))]
pub(crate) async fn get_verified_query(
    State(state): State<AppState>,
    _principal: Principal,
    _ws: WorkspaceContext,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<VerifiedQueryDef>>, AppError> {
    let typed_id = VerifiedQueryId::new(id);
    let row = state
        .store
        .get_verified_query(&typed_id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("VerifiedQuery"))?;
    Ok(ApiResponse::of(row))
}

#[utoipa::path(
    post,
    path = "/api/verified-queries/{id}/transition-status",
    params(("id" = String, Path, description = "Verified query id")),
    request_body = TransitionVerifiedQueryStatusRequest,
    responses(
        (status = 200, description = "Status transitioned", body = VerifiedQueryDef),
        (status = 403, description = "Designer role required",
            body = inline(crate::openapi::ErrorResponse)),
        (status = 404, description = "Not found",
            body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "VerifiedQueries",
)]
#[tracing::instrument(skip(state, principal, req))]
pub(crate) async fn transition_verified_query_status(
    State(state): State<AppState>,
    principal: Principal,
    _ws: WorkspaceContext,
    Path(id): Path<String>,
    Json(req): Json<TransitionVerifiedQueryStatusRequest>,
) -> Result<Json<ApiResponse<VerifiedQueryDef>>, AppError> {
    principal.require_designer()?;
    let typed_id = VerifiedQueryId::new(id);
    let updated = state
        .store
        .transition_verified_query_status(&typed_id, req.status)
        .await
        .map_err(AppError::from)?;
    Ok(ApiResponse::of(updated))
}

#[utoipa::path(
    delete,
    path = "/api/verified-queries/{id}",
    params(("id" = String, Path, description = "Verified query id")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 403, description = "Designer role required",
            body = inline(crate::openapi::ErrorResponse)),
        (status = 404, description = "Not found",
            body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "VerifiedQueries",
)]
#[tracing::instrument(skip(state, principal))]
pub(crate) async fn delete_verified_query(
    State(state): State<AppState>,
    principal: Principal,
    _ws: WorkspaceContext,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    principal.require_designer()?;
    let typed_id = VerifiedQueryId::new(id);
    let removed = state
        .store
        .delete_verified_query(&typed_id)
        .await
        .map_err(AppError::from)?;
    if !removed {
        return Err(AppError::not_found("VerifiedQuery"));
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Φ11.5 — best-effort embedding of the canonical promotion
/// question.
///
/// Returns `None` (silent fallthrough) when:
///
/// - the brain wasn't built with an embedder
///   (cold-start / test fixtures),
/// - the embed call fails (logged at `warn`; promotion still
///   succeeds with an empty `embedding` and the row falls into
///   the trigram-retriever path until a re-promote refills it).
///
/// The embedding role is `Document` — verified queries ARE the
/// retrieved corpus the Brain matches *user* questions against;
/// at lookup time the user query gets the `Query` role.
async fn embed_question_for_verified_query(
    state: &crate::state::AppState,
    question: &str,
) -> Option<Vec<f32>> {
    let memory = state.memory.as_ref()?;
    match memory
        .embedder()
        .embed(
            question,
            "Represent the analytical question for retrieval",
            ox_memory::EmbeddingRole::Document,
        )
        .await
    {
        Ok(v) => Some(v),
        Err(err) => {
            tracing::warn!(
                error = %err,
                "verified-query embedding failed; row promoted without semantic vector",
            );
            None
        }
    }
}
