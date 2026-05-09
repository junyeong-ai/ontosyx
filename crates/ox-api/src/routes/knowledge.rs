use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use ox_store::store::CursorParams;
use ox_store::{KnowledgeEntry, KnowledgeKind, KnowledgeStatus};

use crate::error::AppError;
use crate::principal::Principal;
use crate::response::ApiResponse;
use crate::state::AppState;
use crate::workspace::WorkspaceContext;

// ---------------------------------------------------------------------------
// POST /api/knowledge — create a knowledge entry
// ---------------------------------------------------------------------------

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateKnowledgeEntryRequest {
    pub ontology_name: String,
    pub kind: KnowledgeKind,
    pub title: String,
    pub content: String,
    #[serde(default)]
    #[schema(value_type = HashMap<String, Object>, additional_properties)]
    pub structured_data: serde_json::Value,
    #[serde(default)]
    pub affected_labels: Vec<String>,
    pub ontology_version_min: Option<i32>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct KnowledgeBulkReviewResponse {
    pub reviewed: u64,
}

#[utoipa::path(
    post,
    path = "/api/knowledge",
    request_body = CreateKnowledgeEntryRequest,
    responses(
        (status = 200, description = "Entry created", body = KnowledgeEntry),
        (status = 400, description = "Validation failure"),
    ),
    security(("api_key" = [])),
    tag = "Knowledge",
)]
pub(crate) async fn create_knowledge(
    State(state): State<AppState>,
    principal: Principal,
    ws: WorkspaceContext,
    Json(req): Json<CreateKnowledgeEntryRequest>,
) -> Result<Json<ApiResponse<KnowledgeEntry>>, AppError> {
    principal.require_designer()?;
    if req.title.trim().is_empty() || req.title.len() > 500 {
        return Err(AppError::text_length_out_of_range("title", 1, 500));
    }
    if req.content.trim().is_empty() {
        return Err(AppError::required_field_empty("content"));
    }
    if req.ontology_name.trim().is_empty() {
        return Err(AppError::required_field_empty("ontology_name"));
    }

    // Server-side content_hash computation (never trust client)
    let hash = ox_brain::knowledge_util::content_hash(&req.ontology_name, &req.content);

    let tokens = crate::tokenizer_publish::tokenize_for_workspace(
        &state,
        ws.workspace_id,
        &format!("{}\n{}", req.title, req.content),
    )
    .await;

    let entry = KnowledgeEntry {
        id: Uuid::new_v4(),
        workspace_id: ws.workspace_id,
        ontology_name: req.ontology_name,
        ontology_version_min: req.ontology_version_min.unwrap_or(1),
        ontology_version_max: None,
        kind: req.kind,
        status: KnowledgeStatus::Draft,
        confidence: 1.0, // admin-created
        title: req.title,
        content: req.content,
        structured_data: req.structured_data,
        embedding: None,
        version_checked: req.ontology_version_min.unwrap_or(1),
        content_hash: hash,
        source_execution_ids: vec![],
        source_session_id: None,
        affected_labels: req.affected_labels,
        affected_properties: vec![],
        created_by: principal.id,
        reviewed_by: None,
        reviewed_at: None,
        review_notes: None,
        use_count: 0,
        last_used_at: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        tokenized_text: tokens.tokenized_text,
        tokenizer_dict_fingerprint: tokens.tokenizer_dict_fingerprint,
    };

    state.store.create_knowledge_entry(&entry).await?;
    Ok(ApiResponse::of(entry))
}

// ---------------------------------------------------------------------------
// GET /api/knowledge — list knowledge entries
// ---------------------------------------------------------------------------

#[derive(Deserialize, utoipa::IntoParams)]
pub struct KnowledgeListQuery {
    pub ontology_name: Option<String>,
    pub kind: Option<KnowledgeKind>,
    pub status: Option<KnowledgeStatus>,
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/knowledge",
    params(KnowledgeListQuery),
    responses((status = 200, description = "Knowledge entries", body = crate::openapi::KnowledgeEntryPage)),
    security(("api_key" = [])),
    tag = "Knowledge",
)]
pub(crate) async fn list_knowledge(
    State(state): State<AppState>,
    _principal: Principal,
    Query(q): Query<KnowledgeListQuery>,
) -> Result<Json<ApiResponse<Vec<KnowledgeEntry>>>, AppError> {
    let pagination = CursorParams {
        limit: q.limit.unwrap_or(50),
        cursor: q.cursor,
    };
    let page = state
        .store
        .list_knowledge_entries(
            q.ontology_name.as_deref(),
            q.kind.map(KnowledgeKind::as_str),
            q.status.map(KnowledgeStatus::as_str),
            &pagination,
        )
        .await?;
    Ok(ApiResponse::page(page))
}

// ---------------------------------------------------------------------------
// GET /api/knowledge/{id} — get a knowledge entry
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/knowledge/{id}",
    params(("id" = Uuid, Path, description = "Knowledge entry ID")),
    responses(
        (status = 200, description = "Knowledge entry", body = KnowledgeEntry),
        (status = 404, description = "Entry not found"),
    ),
    security(("api_key" = [])),
    tag = "Knowledge",
)]
pub(crate) async fn get_knowledge(
    State(state): State<AppState>,
    _principal: Principal,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<KnowledgeEntry>>, AppError> {
    let entry = state
        .store
        .get_knowledge_entry(id)
        .await?
        .ok_or_else(|| AppError::not_found("Knowledge entry not found"))?;
    Ok(ApiResponse::of(entry))
}

// ---------------------------------------------------------------------------
// PATCH /api/knowledge/{id} — update a knowledge entry
// ---------------------------------------------------------------------------

#[derive(Deserialize, utoipa::ToSchema)]
pub struct UpdateKnowledgeEntryRequest {
    pub title: String,
    pub content: String,
    #[serde(default)]
    #[schema(value_type = HashMap<String, Object>, additional_properties)]
    pub structured_data: serde_json::Value,
    #[serde(default)]
    pub affected_labels: Vec<String>,
    #[serde(default)]
    pub affected_properties: Option<Vec<String>>,
}

#[utoipa::path(
    patch,
    path = "/api/knowledge/{id}",
    params(("id" = Uuid, Path, description = "Knowledge entry ID")),
    request_body = UpdateKnowledgeEntryRequest,
    responses((status = 204, description = "Entry updated")),
    security(("api_key" = [])),
    tag = "Knowledge",
)]
pub(crate) async fn update_knowledge(
    State(state): State<AppState>,
    principal: Principal,
    ws: WorkspaceContext,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateKnowledgeEntryRequest>,
) -> Result<StatusCode, AppError> {
    principal.require_designer()?;
    let tokens = crate::tokenizer_publish::tokenize_for_workspace(
        &state,
        ws.workspace_id,
        &format!("{}\n{}", req.title, req.content),
    )
    .await;
    state
        .store
        .update_knowledge_entry(
            id,
            &req.title,
            &req.content,
            &req.structured_data,
            &req.affected_labels,
            &req.affected_properties.unwrap_or_default(),
            &tokens.tokenized_text,
            &tokens.tokenizer_dict_fingerprint,
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// DELETE /api/knowledge/{id} — delete a knowledge entry
// ---------------------------------------------------------------------------

#[utoipa::path(
    delete,
    path = "/api/knowledge/{id}",
    params(("id" = Uuid, Path, description = "Knowledge entry ID")),
    responses(
        (status = 204, description = "Entry deleted"),
        (status = 404, description = "Entry not found"),
    ),
    security(("api_key" = [])),
    tag = "Knowledge",
)]
pub(crate) async fn delete_knowledge(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    principal.require_admin()?;
    let deleted = state.store.delete_knowledge_entry(id).await?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::not_found("Knowledge entry"))
    }
}

// ---------------------------------------------------------------------------
// PATCH /api/knowledge/{id}/status — update status (admin review)
// ---------------------------------------------------------------------------

#[derive(Deserialize, utoipa::ToSchema)]
pub struct UpdateKnowledgeStatusRequest {
    pub status: KnowledgeStatus,
    pub review_notes: Option<String>,
}

#[utoipa::path(
    patch,
    path = "/api/knowledge/{id}/status",
    params(("id" = Uuid, Path, description = "Knowledge entry ID")),
    request_body = UpdateKnowledgeStatusRequest,
    responses(
        (status = 204, description = "Status updated"),
        (status = 400, description = "Invalid status"),
    ),
    security(("api_key" = [])),
    tag = "Knowledge",
)]
pub(crate) async fn update_status(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateKnowledgeStatusRequest>,
) -> Result<StatusCode, AppError> {
    principal.require_admin()?;
    state
        .store
        .update_knowledge_status(
            id,
            req.status,
            principal.user_uuid().ok(),
            req.review_notes.as_deref(),
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// GET /api/knowledge/stale — list stale entries for admin review
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/knowledge/stale",
    responses((status = 200, description = "Stale entries awaiting review", body = crate::openapi::KnowledgeEntryPage)),
    security(("api_key" = [])),
    tag = "Knowledge",
)]
pub(crate) async fn list_stale(
    State(state): State<AppState>,
    principal: Principal,
) -> Result<Json<ApiResponse<Vec<KnowledgeEntry>>>, AppError> {
    principal.require_admin()?;
    let page = state
        .store
        .list_knowledge_entries(
            None,
            None,
            Some(KnowledgeStatus::Stale.as_str()),
            &CursorParams {
                cursor: None,
                limit: 100,
            },
        )
        .await?;
    Ok(ApiResponse::page(page))
}

// ---------------------------------------------------------------------------
// GET /api/knowledge/stats — knowledge base statistics
// ---------------------------------------------------------------------------

#[derive(Serialize, utoipa::ToSchema)]
pub struct KnowledgeStats {
    pub total: i64,
    pub by_status: std::collections::HashMap<String, i64>,
    pub by_kind: std::collections::HashMap<String, i64>,
}

#[utoipa::path(
    get,
    path = "/api/knowledge/stats",
    responses((status = 200, description = "Knowledge counts", body = KnowledgeStats)),
    security(("api_key" = [])),
    tag = "Knowledge",
)]
pub(crate) async fn knowledge_stats(
    State(state): State<AppState>,
    principal: Principal,
) -> Result<Json<ApiResponse<KnowledgeStats>>, AppError> {
    principal.require_admin()?;
    let rows = state.store.count_knowledge_by_status_kind().await?;

    let mut total = 0i64;
    let mut by_status: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    let mut by_kind: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    for (status, kind, cnt) in &rows {
        total += cnt;
        *by_status.entry(status.clone()).or_default() += cnt;
        *by_kind.entry(kind.clone()).or_default() += cnt;
    }

    Ok(ApiResponse::of(KnowledgeStats {
        total,
        by_status,
        by_kind,
    }))
}

// ---------------------------------------------------------------------------
// POST /api/knowledge/bulk-review — bulk approve/deprecate
// ---------------------------------------------------------------------------

#[derive(Deserialize, utoipa::ToSchema)]
pub struct BulkReviewApprovalsRequest {
    pub ids: Vec<Uuid>,
    pub status: KnowledgeStatus,
    pub review_notes: Option<String>,
}

#[utoipa::path(
    post,
    path = "/api/knowledge/bulk-review",
    request_body = BulkReviewApprovalsRequest,
    responses(
        (status = 200, description = "Bulk review applied", body = KnowledgeBulkReviewResponse),
        (status = 400, description = "Invalid status or batch too large"),
    ),
    security(("api_key" = [])),
    tag = "Knowledge",
)]
pub(crate) async fn bulk_review(
    State(state): State<AppState>,
    principal: Principal,
    Json(req): Json<BulkReviewApprovalsRequest>,
) -> Result<Json<ApiResponse<KnowledgeBulkReviewResponse>>, AppError> {
    principal.require_admin()?;
    if req.ids.len() > 100 {
        return Err(AppError::bulk_limit_exceeded(100));
    }
    let reviewer_id = principal.user_uuid().ok();
    let mut count = 0u64;
    for id in &req.ids {
        if state
            .store
            .update_knowledge_status(*id, req.status, reviewer_id, req.review_notes.as_deref())
            .await
            .is_ok()
        {
            count += 1;
        }
    }
    Ok(ApiResponse::of(KnowledgeBulkReviewResponse {
        reviewed: count,
    }))
}
