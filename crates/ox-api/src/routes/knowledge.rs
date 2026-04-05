use axum::Json;
use axum::extract::{Path, Query, State};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use ox_store::store::CursorParams;
use ox_store::{CursorPage, KnowledgeEntry};

use crate::error::AppError;
use crate::principal::Principal;
use crate::state::AppState;

// ---------------------------------------------------------------------------
// POST /api/knowledge — create a knowledge entry
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct KnowledgeCreateRequest {
    pub ontology_name: String,
    pub kind: String,
    pub title: String,
    pub content: String,
    #[serde(default)]
    pub structured_data: serde_json::Value,
    #[serde(default)]
    pub affected_labels: Vec<String>,
    pub ontology_version_min: Option<i32>,
}

const VALID_KINDS: &[&str] = &["correction", "hint"];
const VALID_STATUSES: &[&str] = &["draft", "approved", "stale", "deprecated"];

pub(crate) async fn create_knowledge(
    State(state): State<AppState>,
    principal: Principal,
    Json(req): Json<KnowledgeCreateRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    principal.require_designer()?;
    if !VALID_KINDS.contains(&req.kind.as_str()) {
        return Err(AppError::bad_request(format!(
            "Invalid kind '{}'. Must be one of: {}",
            req.kind,
            VALID_KINDS.join(", ")
        )));
    }
    if req.title.trim().is_empty() || req.title.len() > 500 {
        return Err(AppError::bad_request("Title must be 1-500 characters"));
    }
    if req.content.trim().is_empty() {
        return Err(AppError::bad_request("Content must not be empty"));
    }
    if req.ontology_name.trim().is_empty() {
        return Err(AppError::bad_request("Ontology name must not be empty"));
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
    Ok(Json(serde_json::json!({ "id": entry.id })))
}

// ---------------------------------------------------------------------------
// GET /api/knowledge — list knowledge entries
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct KnowledgeListQuery {
    pub ontology_name: Option<String>,
    pub kind: Option<String>,
    pub status: Option<String>,
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}

pub(crate) async fn list_knowledge(
    State(state): State<AppState>,
    _principal: Principal,
    Query(q): Query<KnowledgeListQuery>,
) -> Result<Json<CursorPage<KnowledgeEntry>>, AppError> {
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
    Ok(Json(page))
}

// ---------------------------------------------------------------------------
// GET /api/knowledge/{id} — get a knowledge entry
// ---------------------------------------------------------------------------

pub(crate) async fn get_knowledge(
    State(state): State<AppState>,
    _principal: Principal,
    Path(id): Path<Uuid>,
) -> Result<Json<KnowledgeEntry>, AppError> {
    let entry = state
        .store
        .get_knowledge_entry(id)
        .await?
        .ok_or_else(|| AppError::not_found("Knowledge entry not found"))?;
    Ok(Json(entry))
}

// ---------------------------------------------------------------------------
// PATCH /api/knowledge/{id} — update a knowledge entry
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct KnowledgeUpdateRequest {
    pub title: String,
    pub content: String,
    #[serde(default)]
    pub structured_data: serde_json::Value,
    #[serde(default)]
    pub affected_labels: Vec<String>,
    #[serde(default)]
    pub affected_properties: Option<Vec<String>>,
}

pub(crate) async fn update_knowledge(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
    Json(req): Json<KnowledgeUpdateRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
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
    Ok(Json(serde_json::json!({ "ok": true })))
}

// ---------------------------------------------------------------------------
// DELETE /api/knowledge/{id} — delete a knowledge entry
// ---------------------------------------------------------------------------

pub(crate) async fn delete_knowledge(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    principal.require_admin()?;
    let deleted = state.store.delete_knowledge_entry(id).await?;
    Ok(Json(serde_json::json!({ "deleted": deleted })))
}

// ---------------------------------------------------------------------------
// PATCH /api/knowledge/{id}/status — update status (admin review)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct KnowledgeStatusRequest {
    pub status: String,
    pub review_notes: Option<String>,
}

pub(crate) async fn update_status(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
    Json(req): Json<KnowledgeStatusRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    principal.require_admin()?;
    if !VALID_STATUSES.contains(&req.status.as_str()) {
        return Err(AppError::bad_request(format!(
            "Invalid status '{}'. Must be one of: {}",
            req.status,
            VALID_STATUSES.join(", ")
        )));
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
    Ok(Json(serde_json::json!({ "ok": true })))
}

// ---------------------------------------------------------------------------
// GET /api/knowledge/stale — list stale entries for admin review
// ---------------------------------------------------------------------------

pub(crate) async fn list_stale(
    State(state): State<AppState>,
    principal: Principal,
) -> Result<Json<CursorPage<KnowledgeEntry>>, AppError> {
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
    Ok(Json(page))
}

// ---------------------------------------------------------------------------
// GET /api/knowledge/stats — knowledge base statistics
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct KnowledgeStats {
    pub total: i64,
    pub by_status: serde_json::Value,
    pub by_kind: serde_json::Value,
}

pub(crate) async fn knowledge_stats(
    State(state): State<AppState>,
    principal: Principal,
) -> Result<Json<KnowledgeStats>, AppError> {
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

    Ok(Json(KnowledgeStats {
        total,
        by_status: serde_json::to_value(by_status).unwrap_or_default(),
        by_kind: serde_json::to_value(by_kind).unwrap_or_default(),
    }))
}

// ---------------------------------------------------------------------------
// POST /api/knowledge/bulk-review — bulk approve/deprecate
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct BulkReviewRequest {
    pub ids: Vec<Uuid>,
    pub status: String,
    pub review_notes: Option<String>,
}

pub(crate) async fn bulk_review(
    State(state): State<AppState>,
    principal: Principal,
    Json(req): Json<BulkReviewRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    principal.require_admin()?;
    if !VALID_STATUSES.contains(&req.status.as_str()) {
        return Err(AppError::bad_request(format!(
            "Invalid status '{}'. Must be one of: {}",
            req.status,
            VALID_STATUSES.join(", ")
        )));
    }
    if req.ids.len() > 100 {
        return Err(AppError::bad_request("Maximum 100 entries per bulk review"));
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
    Ok(Json(serde_json::json!({ "reviewed": count })))
}
