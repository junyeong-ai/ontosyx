//! Ambiguity admin surface — list, detail, resolve, revoke.
//!
//! The agent's `resolve-ambiguity` tool already writes through the
//! same `AmbiguityStore` trait; these endpoints give human admins a
//! parallel path so a steward can review or override an
//! auto-resolution the agent landed, or pick up a context the agent
//! hasn't touched yet.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use ox_ontology::ambiguity::{
    AmbiguityContext, AmbiguityId, AmbiguityMapping, AmbiguityResolution,
};

use crate::error::AppError;
use crate::principal::Principal;
use crate::response::ApiResponse;
use crate::state::AppState;

// ---------------------------------------------------------------------------
// Shared wire shape
//
// `AmbiguityContext` / `AmbiguityResolution` come from `ox-ontology`,
// which does not carry a `utoipa` dependency (the crate graph keeps
// schema-generation deps out of the IR layer). We therefore skip the
// `ToSchema` derives here and declare `body = Object` on the utoipa
// paths — the handwritten openapi.json stays accurate via the return
// type, but utoipa only sees a free-form object.
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct AmbiguitySummary {
    pub context: AmbiguityContext,
    /// The currently-active resolution, if any. Admins treat an
    /// absent resolution as "needs attention".
    pub active_resolution: Option<AmbiguityResolution>,
}

#[derive(Debug, Serialize)]
pub struct AmbiguityListResponse {
    pub items: Vec<AmbiguitySummary>,
}

// ---------------------------------------------------------------------------
// GET /api/ambiguities  (workspace-scoped via RLS)
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/ambiguities",
    responses(
        (status = 200, description = "All contexts + their active resolution", body = Object),
    ),
    security(("api_key" = [])),
    tag = "Ambiguity",
)]
pub(crate) async fn list_ambiguities(
    State(state): State<AppState>,
    _principal: Principal,
) -> Result<Json<ApiResponse<AmbiguityListResponse>>, AppError> {
    let contexts = state
        .store
        .list_ambiguity_contexts_in_workspace()
        .await
        .map_err(AppError::from)?;

    let mut items = Vec::with_capacity(contexts.len());
    for ctx in contexts {
        let active = state
            .store
            .get_active_ambiguity_resolution(&ctx.source_id, &ctx.column)
            .await
            .map_err(AppError::from)?;
        items.push(AmbiguitySummary {
            context: ctx,
            active_resolution: active,
        });
    }

    Ok(ApiResponse::of(AmbiguityListResponse { items }))
}

// ---------------------------------------------------------------------------
// GET /api/ambiguities/{id}  — context + full resolution history
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct AmbiguityDetailResponse {
    pub context: AmbiguityContext,
    /// Resolution chain, newest first. The first row whose
    /// `revoked_at` is null is the currently-active one; a fully
    /// revoked chain leaves the context "unresolved".
    pub history: Vec<AmbiguityResolution>,
}

#[utoipa::path(
    get,
    path = "/api/ambiguities/{id}",
    params(("id" = Uuid, Path, description = "AmbiguityContext id")),
    responses(
        (status = 200, description = "Context + resolution history", body = Object),
        (status = 404, description = "Not found"),
    ),
    security(("api_key" = [])),
    tag = "Ambiguity",
)]
pub(crate) async fn get_ambiguity(
    State(state): State<AppState>,
    _principal: Principal,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<AmbiguityDetailResponse>>, AppError> {
    let ctx_id = AmbiguityId::new(id.to_string());
    let ctx = state
        .store
        .get_ambiguity_context(&ctx_id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("ambiguity context"))?;
    let history = state
        .store
        .list_ambiguity_resolutions(&ctx_id)
        .await
        .map_err(AppError::from)?;
    Ok(ApiResponse::of(AmbiguityDetailResponse {
        context: ctx,
        history,
    }))
}

// ---------------------------------------------------------------------------
// POST /api/ambiguities/{id}/resolve  — create a new resolution
//
// The store impl is responsible for atomically revoking the prior
// active resolution (if any) and writing the new one with
// `supersedes` set. This endpoint only carries the semantic payload
// (the `AmbiguityMapping`).
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ResolveAmbiguityRequest {
    pub mapping: AmbiguityMapping,
}

#[utoipa::path(
    post,
    path = "/api/ambiguities/{id}/resolve",
    params(("id" = Uuid, Path, description = "AmbiguityContext id")),
    request_body = Object,
    responses(
        (status = 201, description = "Resolution recorded", body = Object),
        (status = 404, description = "Context not found"),
    ),
    security(("api_key" = [])),
    tag = "Ambiguity",
)]
pub(crate) async fn resolve_ambiguity(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
    Json(req): Json<ResolveAmbiguityRequest>,
) -> Result<(StatusCode, Json<ApiResponse<AmbiguityResolution>>), AppError> {
    principal.require_designer()?;

    let ctx_id = AmbiguityId::new(id.to_string());
    let ctx = state
        .store
        .get_ambiguity_context(&ctx_id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("ambiguity context"))?;

    let mut resolution =
        AmbiguityResolution::new(ctx_id, ctx.detection_source_hash, req.mapping);
    resolution.resolved_by_user_id = principal.user_uuid().ok();

    let written = state
        .store
        .create_ambiguity_resolution(resolution)
        .await
        .map_err(AppError::from)?;

    Ok((StatusCode::CREATED, ApiResponse::of(written)))
}

// ---------------------------------------------------------------------------
// POST /api/ambiguities/{id}/revoke  — revoke without replacement
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct RevokeAmbiguityResponse {
    pub revoked: bool,
}

#[utoipa::path(
    post,
    path = "/api/ambiguities/{id}/revoke",
    params(("id" = Uuid, Path, description = "AmbiguityContext id")),
    responses(
        (status = 200, description = "Revoked (or noop if no active)", body = Object),
    ),
    security(("api_key" = [])),
    tag = "Ambiguity",
)]
pub(crate) async fn revoke_ambiguity(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<RevokeAmbiguityResponse>>, AppError> {
    principal.require_designer()?;
    let ctx_id = AmbiguityId::new(id.to_string());
    let revoked = state
        .store
        .revoke_active_ambiguity_resolution(&ctx_id)
        .await
        .map_err(AppError::from)?;
    Ok(ApiResponse::of(RevokeAmbiguityResponse { revoked }))
}

// ---------------------------------------------------------------------------
// POST /api/ambiguities/bulk-revoke  — revoke many active resolutions
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct BulkRevokeAmbiguitiesRequest {
    /// Context ids to revoke. Capped at 100 per call by the
    /// `bulk_limit_exceeded` typed gate; clients split into
    /// multiple calls when the cohort is larger.
    pub ids: Vec<Uuid>,
}

#[derive(Debug, Serialize)]
pub struct BulkRevokeAmbiguitiesResponse {
    /// Count of contexts that actually transitioned. Contexts
    /// without an active resolution are silently skipped (matches
    /// single-id semantics), so `revoked` may be less than
    /// `ids.len()` when the cohort overlaps already-revoked or
    /// unresolved contexts.
    pub revoked: u64,
}

#[utoipa::path(
    post,
    path = "/api/ambiguities/bulk-revoke",
    request_body = Object,
    responses(
        (status = 200, description = "Bulk revoke recorded", body = Object),
        (status = 400, description = "Empty or oversized ids list", body = crate::openapi::ErrorResponse),
        (status = 403, description = "Designer required", body = crate::openapi::ErrorResponse),
    ),
    security(("api_key" = [])),
    tag = "Ambiguity",
)]
pub(crate) async fn bulk_revoke_ambiguities(
    State(state): State<AppState>,
    principal: Principal,
    Json(req): Json<BulkRevokeAmbiguitiesRequest>,
) -> Result<Json<ApiResponse<BulkRevokeAmbiguitiesResponse>>, AppError> {
    principal.require_designer()?;
    if req.ids.is_empty() {
        return Err(AppError::required_field_empty("ids"));
    }
    if req.ids.len() > 100 {
        return Err(AppError::bulk_limit_exceeded(100));
    }
    let ctx_ids: Vec<AmbiguityId> = req
        .ids
        .iter()
        .map(|u| AmbiguityId::new(u.to_string()))
        .collect();
    let revoked = state
        .store
        .bulk_revoke_active_ambiguity_resolutions(&ctx_ids)
        .await
        .map_err(AppError::from)?;
    Ok(ApiResponse::of(BulkRevokeAmbiguitiesResponse { revoked }))
}
