use std::collections::HashMap;

use axum::Json;
use axum::extract::{Path, State};
use chrono::Utc;
use serde::Deserialize;
use tracing::{error, info};
use uuid::Uuid;

use ox_query_ir::query::QueryResult;
use ox_core::types::PropertyValue;
use ox_store::SavedReport;
use ox_store::store::CursorParams;

use crate::error::AppError;
use crate::principal::Principal;
use crate::response::ApiResponse;
use crate::state::AppState;

// ---------------------------------------------------------------------------
// POST /api/reports — create a new saved report
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ReportCreateRequest {
    pub ontology_lineage_id: String,
    pub title: String,
    pub description: Option<String>,
    pub query_template: String,
    #[serde(default = "default_parameters")]
    pub parameters: serde_json::Value,
    pub widget_type: Option<String>,
    #[serde(default)]
    pub is_public: bool,
}

fn default_parameters() -> serde_json::Value {
    serde_json::json!([])
}

/// Best-effort resolution of the current-version IR for a lineage id.
/// Used by read-only paths that want to bind `GRAPH_ONTOLOGY` for the
/// runtime's OntologyValidator but don't want to hard-fail when the
/// lineage has been archived or was never committed — those callers
/// fall back to safety + workspace-scope validation only.
async fn resolve_lineage_current_ir(
    state: &AppState,
    lineage_id: &str,
) -> Option<std::sync::Arc<ox_ontology::ir::OntologyIR>> {
    let identity = state.store.find_ontology_by_lineage(lineage_id).await.ok()??;
    let version = state.store.get_current_version(identity.id).await.ok()??;
    let ir = state.store.get_ontology_ir(version.id).await.ok()??;
    Some(std::sync::Arc::new(ir))
}

pub(crate) async fn create_report(
    State(state): State<AppState>,
    principal: Principal,
    Json(req): Json<ReportCreateRequest>,
) -> Result<Json<ApiResponse<SavedReport>>, AppError> {
    if req.title.trim().is_empty() {
        return Err(AppError::bad_request("Report title must not be empty"));
    }
    if req.query_template.trim().is_empty() {
        return Err(AppError::bad_request("Query template must not be empty"));
    }

    let now = Utc::now();
    let report = SavedReport {
        id: Uuid::new_v4(),
        user_id: principal.id.clone(),
        ontology_lineage_id: req.ontology_lineage_id,
        title: req.title,
        description: req.description,
        query_template: req.query_template,
        parameters: req.parameters,
        widget_type: req.widget_type,
        is_public: req.is_public,
        created_at: now,
        updated_at: now,
    };

    state
        .store
        .create_report(&report)
        .await
        .map_err(AppError::from)?;

    Ok(ApiResponse::of(report))
}

// ---------------------------------------------------------------------------
// GET /api/reports?ontology_lineage_id=... — list reports for an ontology
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ReportListParams {
    pub ontology_lineage_id: String,
    #[serde(default)]
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}

pub(crate) async fn list_reports(
    State(state): State<AppState>,
    principal: Principal,
    axum::extract::Query(query): axum::extract::Query<ReportListParams>,
) -> Result<Json<ApiResponse<Vec<SavedReport>>>, AppError> {
    let pagination = CursorParams {
        limit: query.limit.unwrap_or(50),
        cursor: query.cursor,
    };
    let page = state
        .store
        .list_reports(&principal.id, &query.ontology_lineage_id, &pagination)
        .await
        .map_err(AppError::from)?;
    Ok(ApiResponse::page(page))
}

// ---------------------------------------------------------------------------
// GET /api/reports/:id — get a single report
// ---------------------------------------------------------------------------

pub(crate) async fn get_report(
    State(state): State<AppState>,
    _principal: Principal,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<SavedReport>>, AppError> {
    let report = state
        .store
        .get_report(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Report"))?;
    Ok(ApiResponse::of(report))
}

// ---------------------------------------------------------------------------
// PATCH /api/reports/:id — update a report
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ReportUpdateRequest {
    pub title: Option<String>,
    pub description: Option<String>,
    pub query_template: Option<String>,
    pub parameters: Option<serde_json::Value>,
    pub widget_type: Option<String>,
    pub is_public: Option<bool>,
}

pub(crate) async fn update_report(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
    Json(req): Json<ReportUpdateRequest>,
) -> Result<Json<ApiResponse<SavedReport>>, AppError> {
    let existing = state
        .store
        .get_report(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Report"))?;
    principal.require_owner(&existing.user_id, "report")?;

    let title = req.title.as_deref().unwrap_or(&existing.title);
    let description = match &req.description {
        Some(d) => Some(d.as_str()),
        None => existing.description.as_deref(),
    };
    let query_template = req
        .query_template
        .as_deref()
        .unwrap_or(&existing.query_template);
    let parameters = req.parameters.as_ref().unwrap_or(&existing.parameters);
    let widget_type = match &req.widget_type {
        Some(wt) => Some(wt.as_str()),
        None => existing.widget_type.as_deref(),
    };
    let is_public = req.is_public.unwrap_or(existing.is_public);

    state
        .store
        .update_report(
            id,
            title,
            description,
            query_template,
            parameters,
            widget_type,
            is_public,
        )
        .await
        .map_err(AppError::from)?;

    // Re-fetch to return the updated record
    let updated = state
        .store
        .get_report(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Report"))?;

    Ok(ApiResponse::of(updated))
}

// ---------------------------------------------------------------------------
// DELETE /api/reports/:id — delete a report
// ---------------------------------------------------------------------------

pub(crate) async fn delete_report(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
) -> Result<axum::http::StatusCode, AppError> {
    let report = state
        .store
        .get_report(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Report"))?;
    principal.require_owner(&report.user_id, "report")?;

    let deleted = state
        .store
        .delete_report(id)
        .await
        .map_err(AppError::from)?;

    if deleted {
        Ok(axum::http::StatusCode::NO_CONTENT)
    } else {
        Err(AppError::not_found("Report"))
    }
}

// ---------------------------------------------------------------------------
// POST /api/reports/:id/execute — execute a report with parameter values
// ---------------------------------------------------------------------------

pub(crate) async fn execute_report(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
    Json(params): Json<HashMap<String, serde_json::Value>>,
) -> Result<Json<ApiResponse<QueryResult>>, AppError> {
    let report = state
        .store
        .get_report(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Report"))?;

    // Render template: replace {{param}} with values
    let mut query = report.query_template.clone();
    for (key, value) in &params {
        let placeholder = format!("{{{{{}}}}}", key);
        let val_str = match value {
            serde_json::Value::String(s) => s.clone(),
            v => v.to_string(),
        };
        query = query.replace(&placeholder, &val_str);
    }

    info!(
        user_id = %principal.id,
        report_id = %id,
        "Executing saved report"
    );

    let runtime = state.runtime.as_ref().ok_or_else(AppError::no_runtime)?;

    // Resolve the report's lineage so `GRAPH_ONTOLOGY` is bound for the
    // OntologyValidator. A report whose lineage no longer resolves runs
    // with safety + workspace-scope only (unchanged behaviour); no
    // hard error here — orphan reports are a storage-lifetime concern.
    let ontology = resolve_lineage_current_ir(&state, &report.ontology_lineage_id).await;

    let timeout = state.timeouts.raw_query;
    let empty_params: HashMap<String, PropertyValue> = HashMap::new();
    let exec_fut = runtime.execute_query(&query, &empty_params);
    let wrapped_fut = async {
        match ontology {
            Some(onto) => ox_runtime::GRAPH_ONTOLOGY.scope(onto, exec_fut).await,
            None => exec_fut.await,
        }
    };
    let result = tokio::time::timeout(timeout, wrapped_fut)
        .await
        .map_err(|_| {
            AppError::timeout(format!(
                "Report execution timed out after {}s",
                timeout.as_secs()
            ))
        })?
        .map_err(|e| {
            error!("Report execution failed: {e}");
            AppError::unprocessable(format!("Report execution failed: {e}"))
        })?;

    Ok(ApiResponse::of(result))
}
