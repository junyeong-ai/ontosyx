use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use chrono::Utc;
use serde::Deserialize;
use uuid::Uuid;

use ox_store::{QualityDashboardEntry, QualityResult, QualityRule};

use crate::error::AppError;
use crate::principal::Principal;
use crate::state::AppState;
use crate::workspace::WorkspaceContext;

// ---------------------------------------------------------------------------
// Request / Query types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct CreateRuleRequest {
    pub name: String,
    pub rule_type: String,
    pub target_label: String,
    pub target_property: Option<String>,
    pub threshold: Option<f64>,
    pub cypher_check: Option<String>,
    pub severity: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateRuleRequest {
    pub name: Option<String>,
    pub threshold: Option<f64>,
    pub is_active: Option<bool>,
}

#[derive(Deserialize)]
pub struct ListRulesParams {
    pub target_label: Option<String>,
}

#[derive(Deserialize)]
pub struct LimitParam {
    pub limit: Option<i64>,
}

// ---------------------------------------------------------------------------
// POST /api/quality/rules — create a quality rule
// ---------------------------------------------------------------------------

pub(crate) async fn create_rule(
    State(state): State<AppState>,
    principal: Principal,
    ws: WorkspaceContext,
    Json(req): Json<CreateRuleRequest>,
) -> Result<(StatusCode, Json<QualityRule>), AppError> {
    principal.require_designer()?;

    let rule = QualityRule {
        id: Uuid::new_v4(),
        workspace_id: ws.workspace_id,
        name: req.name,
        description: None,
        rule_type: req.rule_type,
        target_label: req.target_label,
        target_property: req.target_property,
        threshold: req.threshold.unwrap_or(1.0),
        cypher_check: req.cypher_check,
        severity: req.severity.unwrap_or_else(|| "warning".to_string()),
        is_active: true,
        created_by: principal.user_uuid().ok(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    state
        .store
        .create_quality_rule(&rule)
        .await
        .map_err(AppError::from)?;

    Ok((StatusCode::CREATED, Json(rule)))
}

// ---------------------------------------------------------------------------
// GET /api/quality/rules — list quality rules
// ---------------------------------------------------------------------------

pub(crate) async fn list_rules(
    State(state): State<AppState>,
    Query(params): Query<ListRulesParams>,
) -> Result<Json<Vec<QualityRule>>, AppError> {
    let rules = state
        .store
        .list_quality_rules(params.target_label.as_deref())
        .await
        .map_err(AppError::from)?;

    Ok(Json(rules))
}

// ---------------------------------------------------------------------------
// GET /api/quality/rules/:id — get single rule
// ---------------------------------------------------------------------------

pub(crate) async fn get_rule(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<QualityRule>, AppError> {
    let rule = state
        .store
        .get_quality_rule(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Quality rule"))?;

    Ok(Json(rule))
}

// ---------------------------------------------------------------------------
// PATCH /api/quality/rules/:id — update rule
// ---------------------------------------------------------------------------

pub(crate) async fn update_rule(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateRuleRequest>,
) -> Result<Json<QualityRule>, AppError> {
    principal.require_designer()?;

    // Fetch existing rule to merge partial updates
    let existing = state
        .store
        .get_quality_rule(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Quality rule"))?;

    let name = req.name.unwrap_or(existing.name);
    let threshold = req.threshold.unwrap_or(existing.threshold);
    let is_active = req.is_active.unwrap_or(existing.is_active);

    state
        .store
        .update_quality_rule(id, &name, threshold, is_active)
        .await
        .map_err(AppError::from)?;

    // Return the updated rule
    let updated = state
        .store
        .get_quality_rule(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Quality rule"))?;

    Ok(Json(updated))
}

// ---------------------------------------------------------------------------
// DELETE /api/quality/rules/:id — delete rule
// ---------------------------------------------------------------------------

pub(crate) async fn delete_rule(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    principal.require_designer()?;

    let deleted = state
        .store
        .delete_quality_rule(id)
        .await
        .map_err(AppError::from)?;

    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::not_found("Quality rule"))
    }
}

// ---------------------------------------------------------------------------
// GET /api/quality/dashboard — overview of all rules + latest results
// ---------------------------------------------------------------------------

pub(crate) async fn quality_dashboard(
    State(state): State<AppState>,
) -> Result<Json<Vec<QualityDashboardEntry>>, AppError> {
    let entries = state
        .store
        .get_quality_dashboard()
        .await
        .map_err(AppError::from)?;

    Ok(Json(entries))
}

// ---------------------------------------------------------------------------
// GET /api/quality/rules/:id/results — latest results for a rule
// ---------------------------------------------------------------------------

pub(crate) async fn rule_results(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(params): Query<LimitParam>,
) -> Result<Json<Vec<QualityResult>>, AppError> {
    let limit = params.limit.unwrap_or(20);

    let results = state
        .store
        .get_latest_results(id, limit)
        .await
        .map_err(AppError::from)?;

    Ok(Json(results))
}
