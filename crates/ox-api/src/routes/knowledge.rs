use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use ox_store::KnowledgeEntry;
use ox_store::store::CursorParams;

use crate::error::AppError;
use crate::principal::Principal;
use crate::response::ApiResponse;
use crate::state::AppState;

// ---------------------------------------------------------------------------
// POST /api/knowledge — create a knowledge entry
// ---------------------------------------------------------------------------

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateKnowledgeEntryRequest {
    pub ontology_name: String,
    pub kind: String,
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

const VALID_KINDS: &[&str] = &["correction", "hint"];
const VALID_STATUSES: &[&str] = &["draft", "approved", "stale", "deprecated"];

#[utoipa::path(
    post,
    path = "/api/knowledge",
    request_body = CreateKnowledgeEntryRequest,
    responses(
        (status = 200, description = "Entry created", body = Object),
        (status = 400, description = "Validation failure"),
    ),
    security(("api_key" = [])),
    tag = "Knowledge",
)]
pub(crate) async fn create_knowledge(
    State(state): State<AppState>,
    principal: Principal,
    Json(req): Json<CreateKnowledgeEntryRequest>,
) -> Result<Json<ApiResponse<KnowledgeEntry>>, AppError> {
    principal.require_designer()?;
    if !VALID_KINDS.contains(&req.kind.as_str()) {
        return Err(AppError::invalid_enum_value(
            "kind",
            req.kind.clone(),
            VALID_KINDS,
        ));
    }
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

    let entry = KnowledgeEntry {
        id: Uuid::new_v4(),
        workspace_id: Uuid::nil(), // RLS default
        ontology_name: req.ontology_name,
        ontology_version_min: req.ontology_version_min.unwrap_or(1),
        ontology_version_max: None,
        kind: req.kind,
        status: "draft".to_string(),
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
    pub kind: Option<String>,
    pub status: Option<String>,
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/knowledge",
    params(KnowledgeListQuery),
    responses((status = 200, description = "Knowledge entries", body = Vec<Object>)),
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
            q.kind.as_deref(),
            q.status.as_deref(),
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
        (status = 200, description = "Knowledge entry", body = Object),
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
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateKnowledgeEntryRequest>,
) -> Result<StatusCode, AppError> {
    principal.require_designer()?;
    state
        .store
        .update_knowledge_entry(
            id,
            &req.title,
            &req.content,
            &req.structured_data,
            &req.affected_labels,
            &req.affected_properties.unwrap_or_default(),
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
    pub status: String,
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
    if !VALID_STATUSES.contains(&req.status.as_str()) {
        return Err(AppError::invalid_enum_value(
            "status",
            req.status.clone(),
            VALID_STATUSES,
        ));
    }
    state
        .store
        .update_knowledge_status(
            id,
            &req.status,
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
    responses((status = 200, description = "Stale entries awaiting review", body = Vec<Object>)),
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
            Some("stale"),
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
    pub status: String,
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
    if !VALID_STATUSES.contains(&req.status.as_str()) {
        return Err(AppError::invalid_enum_value(
            "status",
            req.status.clone(),
            VALID_STATUSES,
        ));
    }
    if req.ids.len() > 100 {
        return Err(AppError::bulk_limit_exceeded(100));
    }
    let reviewer_id = principal.user_uuid().ok();
    let mut count = 0u64;
    for id in &req.ids {
        if state
            .store
            .update_knowledge_status(*id, &req.status, reviewer_id, req.review_notes.as_deref())
            .await
            .is_ok()
        {
            count += 1;
        }
    }
    Ok(ApiResponse::of(KnowledgeBulkReviewResponse { reviewed: count }))
}
