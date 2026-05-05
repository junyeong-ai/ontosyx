//! `/api/insights` — first-class persisted insight artefacts.
//!
//! Saved multi-hop discoveries: a question + the `QueryIR` that
//! answers it + the ontology / registry version it ran against.
//! Re-runnable across schema evolutions; shareable inside a
//! workspace.
//!
//! Server owns identity (UUID v7) and timestamps so concurrent
//! authors never overwrite each other silently — `update_insight`
//! takes an `expected_updated_at` CAS handle and returns
//! `409 Conflict` on stale writes.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};

use ox_query_ir::insight::{InsightDef, InsightId};
use ox_store::store::{CreateInsightInput, UpdateInsightInput};
use ox_store::{CursorPage, CursorParams};

use crate::error::AppError;
use crate::principal::Principal;
use crate::response::ApiResponse;
use crate::state::AppState;
use crate::workspace::WorkspaceContext;

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateInsightRequest {
    #[schema(value_type = Object)]
    pub question: ox_core::i18n::LocalizedText,
    /// Required on the wire — clients always send a `LocalizedText`
    /// payload (`{default: ""}` is acceptable). Mirrors the canonical
    /// `InsightDef.description: LocalizedText` shape; not making the
    /// request DTO optional avoids producer/consumer asymmetry where
    /// the response always carries the field.
    #[schema(value_type = Object)]
    pub description: ox_core::i18n::LocalizedText,
    #[serde(default)]
    pub tags: Vec<String>,
    /// `GlossaryTermId` strings — the typed concept anchors per the
    /// 1-pager's "용어 사전이 다리" axis. Distinct from `tags`
    /// (freeform shorthand) so cross-team filtering by concept stays
    /// stable as tag wording drifts. Empty when no glossary terms
    /// apply.
    #[serde(default)]
    pub concept_anchors: Vec<String>,
    /// Logical query the insight executes. Validated as
    /// `QueryIR` on submit so a malformed IR is rejected up-front.
    #[schema(value_type = Object)]
    pub query_ir: serde_json::Value,
    /// Provenance the insight was originally computed against —
    /// the platform's response basis at save time.
    #[serde(default)]
    #[schema(value_type = Option<Object>)]
    pub original_provenance: Option<serde_json::Value>,
    #[serde(default)]
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct UpdateInsightRequest {
    #[schema(value_type = Object)]
    pub question: ox_core::i18n::LocalizedText,
    /// Required on the wire (see `CreateInsightRequest::description`).
    #[schema(value_type = Object)]
    pub description: ox_core::i18n::LocalizedText,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub concept_anchors: Vec<String>,
    #[schema(value_type = Object)]
    pub query_ir: serde_json::Value,
    #[serde(default)]
    #[schema(value_type = Option<Object>)]
    pub original_provenance: Option<serde_json::Value>,
    #[serde(default)]
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Optimistic-concurrency handle: must match the row's current
    /// `updated_at`. Stale writes return 409 so two concurrent
    /// edits don't silently overwrite each other.
    pub expected_updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct InsightResponse {
    #[schema(value_type = Object)]
    pub insight: InsightDef,
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct ListInsightsQuery {
    /// Restrict to insights authored by `me` — defaults to the
    /// caller. The admin-mode "all insights" view passes `me=false`.
    #[serde(default = "default_me")]
    pub me: bool,
    /// Filter by typed concept anchors (`GlossaryTermId` strings).
    /// Multi-value: `?concept_anchor=gt-x&concept_anchor=gt-y` returns
    /// any insight that carries at least one of those anchors.
    #[serde(default, rename = "concept_anchor")]
    pub concept_anchors: Vec<String>,
    /// Filter by freeform tags. Same multi-value semantics as
    /// `concept_anchor`.
    #[serde(default, rename = "tag")]
    pub tags: Vec<String>,
    /// Cursor returned by the previous call.
    pub cursor: Option<String>,
    /// Page size cap. Server clamps to [1, 100]; defaults to 50.
    pub limit: Option<u32>,
}

fn default_me() -> bool {
    true
}

/// Reject the request when `query_ir` does not deserialise into
/// the canonical `QueryIR` shape. Catches a malformed payload at
/// the edge so every reader can trust the stored row decodes
/// without falling through to a runtime panic.
fn validate_query_ir(value: &serde_json::Value) -> Result<(), AppError> {
    serde_json::from_value::<ox_query_ir::query::QueryIR>(value.clone())
        .map(|_| ())
        .map_err(|e| AppError::query_ir_invalid(e.to_string()))
}

#[utoipa::path(
    post,
    path = "/api/insights",
    request_body = CreateInsightRequest,
    responses(
        (status = 201, description = "Created insight", body = InsightResponse),
        (status = 403, description = "Designer role required",
            body = inline(crate::openapi::ErrorResponse)),
        (status = 422, description = "Invalid query_ir",
            body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Insights",
)]
#[tracing::instrument(skip(state, principal, req))]
pub(crate) async fn create_insight(
    State(state): State<AppState>,
    principal: Principal,
    _ws: WorkspaceContext,
    Json(req): Json<CreateInsightRequest>,
) -> Result<(StatusCode, Json<ApiResponse<InsightResponse>>), AppError> {
    principal.require_designer()?;
    validate_query_ir(&req.query_ir)?;
    let author_id = principal.user_uuid()?;

    let insight = state
        .store
        .create_insight(CreateInsightInput {
            author_id,
            question: req.question,
            description: req.description,
            tags: req.tags,
            concept_anchors: req.concept_anchors,
            query_ir: req.query_ir,
            original_provenance: req.original_provenance,
            expires_at: req.expires_at,
        })
        .await
        .map_err(AppError::from)?;

    record_insight_audit(&state, principal.user_uuid().ok(), "insight.create", &insight.id);

    Ok((
        StatusCode::CREATED,
        ApiResponse::of(InsightResponse { insight }),
    ))
}

#[utoipa::path(
    put,
    path = "/api/insights/{id}",
    params(("id" = String, Path, description = "Insight id")),
    request_body = UpdateInsightRequest,
    responses(
        (status = 200, description = "Updated insight", body = InsightResponse),
        (status = 404, description = "Not found",
            body = inline(crate::openapi::ErrorResponse)),
        (status = 409, description = "Stale update — reload + retry",
            body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Insights",
)]
#[tracing::instrument(skip(state, principal, req))]
pub(crate) async fn update_insight(
    State(state): State<AppState>,
    principal: Principal,
    _ws: WorkspaceContext,
    Path(id): Path<String>,
    Json(req): Json<UpdateInsightRequest>,
) -> Result<Json<ApiResponse<InsightResponse>>, AppError> {
    principal.require_designer()?;
    validate_query_ir(&req.query_ir)?;
    let id = InsightId::new(id);

    let insight = state
        .store
        .update_insight(
            &id,
            UpdateInsightInput {
                question: req.question,
                description: req.description,
                tags: req.tags,
                concept_anchors: req.concept_anchors,
                query_ir: req.query_ir,
                original_provenance: req.original_provenance,
                expires_at: req.expires_at,
                expected_updated_at: req.expected_updated_at,
            },
        )
        .await
        .map_err(AppError::from)?;

    record_insight_audit(&state, principal.user_uuid().ok(), "insight.update", &insight.id);

    Ok(ApiResponse::of(InsightResponse { insight }))
}

#[utoipa::path(
    get,
    path = "/api/insights/{id}",
    params(("id" = String, Path, description = "Insight id")),
    responses(
        (status = 200, description = "Insight detail", body = InsightResponse),
        (status = 404, description = "Not found",
            body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Insights",
)]
#[tracing::instrument(skip(state, _principal))]
pub(crate) async fn get_insight(
    State(state): State<AppState>,
    _principal: Principal,
    _ws: WorkspaceContext,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<InsightResponse>>, AppError> {
    let insight = state
        .store
        .get_insight(&InsightId::new(id))
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Insight"))?;
    Ok(ApiResponse::of(InsightResponse { insight }))
}

#[utoipa::path(
    get,
    path = "/api/insights",
    params(ListInsightsQuery),
    responses(
        (status = 200, description = "Paginated insight list",
            body = Object),
    ),
    security(("api_key" = [])),
    tag = "Insights",
)]
#[tracing::instrument(skip(state, principal))]
pub(crate) async fn list_insights(
    State(state): State<AppState>,
    principal: Principal,
    _ws: WorkspaceContext,
    Query(params): Query<ListInsightsQuery>,
) -> Result<Json<ApiResponse<CursorPage<InsightDef>>>, AppError> {
    let author_filter = if params.me {
        Some(principal.user_uuid()?)
    } else {
        None
    };
    let cursor = CursorParams {
        cursor: params.cursor,
        limit: params.limit.unwrap_or(50),
    };
    let filter = ox_store::store::InsightFilter {
        author_id: author_filter,
        concept_anchors: params.concept_anchors,
        tags: params.tags,
    };
    let page = state
        .store
        .list_insights(&filter, &cursor)
        .await
        .map_err(AppError::from)?;
    Ok(ApiResponse::of(page))
}

#[utoipa::path(
    delete,
    path = "/api/insights/{id}",
    params(("id" = String, Path, description = "Insight id")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 404, description = "Not found",
            body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Insights",
)]
#[tracing::instrument(skip(state, principal))]
pub(crate) async fn delete_insight(
    State(state): State<AppState>,
    principal: Principal,
    _ws: WorkspaceContext,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    principal.require_designer()?;
    let insight_id = InsightId::new(id);
    let removed = state
        .store
        .delete_insight(&insight_id)
        .await
        .map_err(AppError::from)?;
    if !removed {
        return Err(AppError::not_found("Insight"));
    }
    record_insight_audit(&state, principal.user_uuid().ok(), "insight.delete", &insight_id);
    Ok(StatusCode::NO_CONTENT)
}

/// Fire-and-forget audit row for an insight lifecycle event.
/// Spawned through `spawn_scoped` so the workspace context propagates
/// and a logging failure never blocks the user-visible response.
fn record_insight_audit(
    state: &AppState,
    user_id: Option<uuid::Uuid>,
    action: &'static str,
    insight_id: &InsightId,
) {
    let store = std::sync::Arc::clone(&state.store);
    let target_id = insight_id.as_str().to_string();
    crate::spawn_scoped::spawn_scoped(async move {
        if let Err(e) = store
            .record_audit(user_id, action, "insight", Some(&target_id), serde_json::json!({}))
            .await
        {
            tracing::warn!(?e, action, target = %target_id, "insight audit record failed");
        }
    });
}
