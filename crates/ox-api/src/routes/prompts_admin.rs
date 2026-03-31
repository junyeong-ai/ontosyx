use axum::Json;
use axum::extract::{Path, State};
use serde::Deserialize;
use uuid::Uuid;

use ox_store::PromptTemplateRow;

use crate::error::AppError;
use crate::principal::Principal;
use crate::state::AppState;

// ---------------------------------------------------------------------------
// GET /api/admin/prompts — list all prompt templates
// ---------------------------------------------------------------------------

pub(crate) async fn list_prompt_templates(
    State(state): State<AppState>,
    principal: Principal,
) -> Result<Json<Vec<PromptTemplateRow>>, AppError> {
    principal.require_admin()?;
    let rows = state.store.list_prompt_templates(false).await.map_err(AppError::from)?;
    Ok(Json(rows))
}

// ---------------------------------------------------------------------------
// GET /api/admin/prompts/:id — get single prompt template
// ---------------------------------------------------------------------------

pub(crate) async fn get_prompt_template(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
) -> Result<Json<PromptTemplateRow>, AppError> {
    principal.require_admin()?;
    let row = state
        .store
        .get_prompt_template(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Prompt template"))?;
    Ok(Json(row))
}

// ---------------------------------------------------------------------------
// POST /api/admin/prompts — create new prompt template version
// ---------------------------------------------------------------------------

#[derive(Deserialize, utoipa::ToSchema)]
pub struct PromptCreateRequest {
    pub name: String,
    pub version: String,
    pub content: String,
    #[serde(default)]
    pub variables: serde_json::Value,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

pub(crate) async fn create_prompt_template(
    State(state): State<AppState>,
    principal: Principal,
    Json(req): Json<PromptCreateRequest>,
) -> Result<Json<PromptTemplateRow>, AppError> {
    principal.require_admin()?;

    let row = PromptTemplateRow {
        id: Uuid::new_v4(),
        name: req.name,
        version: req.version,
        content: req.content,
        variables: req.variables,
        metadata: req.metadata,
        created_by: principal.id.clone(),
        created_at: chrono::Utc::now(),
        is_active: true,
    };

    state
        .store
        .create_prompt_template(&row)
        .await
        .map_err(AppError::from)?;

    Ok(Json(row))
}

// ---------------------------------------------------------------------------
// PATCH /api/admin/prompts/:id — update prompt template
// ---------------------------------------------------------------------------

#[derive(Deserialize, utoipa::ToSchema)]
pub struct PromptUpdateRequest {
    pub content: Option<String>,
    pub variables: Option<serde_json::Value>,
    pub is_active: Option<bool>,
}

pub(crate) async fn update_prompt_template(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
    Json(req): Json<PromptUpdateRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    principal.require_admin()?;

    let existing = state
        .store
        .get_prompt_template(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Prompt template"))?;

    state
        .store
        .update_prompt_template(
            id,
            req.content.as_deref().unwrap_or(&existing.content),
            req.variables.as_ref().unwrap_or(&existing.variables),
            req.is_active.unwrap_or(existing.is_active),
        )
        .await
        .map_err(AppError::from)?;

    Ok(Json(serde_json::json!({ "status": "ok" })))
}
