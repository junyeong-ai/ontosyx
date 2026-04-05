use axum::Json;
use axum::extract::{Path, State};
use chrono::Utc;
use serde::Deserialize;
use uuid::Uuid;

use ox_store::Dashboard;
use ox_store::store::CursorParams;

use crate::error::AppError;
use crate::principal::Principal;
use crate::state::AppState;
use crate::validation;
use crate::workspace::WorkspaceContext;

// ---------------------------------------------------------------------------
// POST /api/dashboards — create a new dashboard
// ---------------------------------------------------------------------------

#[derive(Deserialize, utoipa::ToSchema)]
pub struct DashboardCreateRequest {
    pub name: String,
    pub description: Option<String>,
}

pub(crate) async fn create_dashboard(
    State(state): State<AppState>,
    principal: Principal,
    ws: WorkspaceContext,
    Json(req): Json<DashboardCreateRequest>,
) -> Result<Json<Dashboard>, AppError> {
    validation::validate_name("name", &req.name)?;

    let dashboard = Dashboard {
        id: Uuid::new_v4(),
        workspace_id: ws.workspace_id,
        user_id: principal.id.clone(),
        name: req.name,
        description: req.description,
        is_public: false,
        share_token: None,
        shared_at: None,
        layout: serde_json::json!([]),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    state
        .store
        .create_dashboard(&dashboard)
        .await
        .map_err(AppError::from)?;

    Ok(Json(dashboard))
}

// ---------------------------------------------------------------------------
// GET /api/dashboards — list dashboards
// ---------------------------------------------------------------------------

pub(crate) async fn list_dashboards(
    State(state): State<AppState>,
    principal: Principal,
    axum::extract::Query(pagination): axum::extract::Query<CursorParams>,
) -> Result<Json<ox_store::store::CursorPage<Dashboard>>, AppError> {
    let is_admin = principal.role.is_admin();
    let page = state
        .store
        .list_dashboards(&principal.id, is_admin, &pagination)
        .await
        .map_err(AppError::from)?;
    Ok(Json(page))
}

// ---------------------------------------------------------------------------
// GET /api/dashboards/:id — get a single dashboard
// ---------------------------------------------------------------------------

pub(crate) async fn get_dashboard(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
) -> Result<Json<Dashboard>, AppError> {
    let dashboard = state
        .store
        .get_dashboard(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Dashboard"))?;

    // Non-admin users can only view their own or public dashboards
    if !principal.role.is_admin() && !dashboard.is_public && dashboard.user_id != principal.id {
        return Err(AppError::not_found("Dashboard"));
    }

    Ok(Json(dashboard))
}

// ---------------------------------------------------------------------------
// PATCH /api/dashboards/:id — update dashboard
// ---------------------------------------------------------------------------

#[derive(Deserialize, utoipa::ToSchema)]
pub struct DashboardUpdateRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    #[schema(value_type = Option<Object>)]
    pub layout: Option<serde_json::Value>,
    pub is_public: Option<bool>,
}

pub(crate) async fn update_dashboard(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
    Json(req): Json<DashboardUpdateRequest>,
) -> Result<Json<Dashboard>, AppError> {
    principal.require_designer()?;

    let mut dashboard = state
        .store
        .get_dashboard(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Dashboard"))?;

    principal.require_owner(&dashboard.user_id, "dashboard")?;

    if let Some(name) = req.name {
        dashboard.name = name;
    }
    if let Some(description) = req.description {
        dashboard.description = Some(description);
    }
    if let Some(layout) = req.layout {
        dashboard.layout = layout;
    }
    if let Some(is_public) = req.is_public {
        dashboard.is_public = is_public;
    }
    dashboard.updated_at = Utc::now();

    state
        .store
        .update_dashboard(
            id,
            &dashboard.name,
            dashboard.description.as_deref(),
            &dashboard.layout,
            dashboard.is_public,
        )
        .await
        .map_err(AppError::from)?;

    Ok(Json(dashboard))
}

// ---------------------------------------------------------------------------
// DELETE /api/dashboards/:id — delete dashboard
// ---------------------------------------------------------------------------

pub(crate) async fn delete_dashboard(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
) -> Result<axum::http::StatusCode, AppError> {
    principal.require_designer()?;

    let dash = state
        .store
        .get_dashboard(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Dashboard"))?;
    principal.require_owner(&dash.user_id, "dashboard")?;

    let deleted = state
        .store
        .delete_dashboard(id)
        .await
        .map_err(AppError::from)?;

    if deleted {
        Ok(axum::http::StatusCode::NO_CONTENT)
    } else {
        Err(AppError::not_found("Dashboard"))
    }
}

// ---------------------------------------------------------------------------
// POST /api/dashboards/:id/widgets — add a widget
// ---------------------------------------------------------------------------

#[derive(Deserialize, utoipa::ToSchema)]
pub struct WidgetCreateRequest {
    pub title: String,
    pub widget_type: String,
    pub query: Option<String>,
    #[serde(default)]
    pub widget_spec: serde_json::Value,
    #[serde(default = "default_position")]
    pub position: serde_json::Value,
    pub refresh_interval_secs: Option<i32>,
    #[schema(value_type = Option<Object>)]
    pub thresholds: Option<serde_json::Value>,
}

fn default_position() -> serde_json::Value {
    serde_json::json!({"x": 0, "y": 0, "w": 6, "h": 4})
}

pub(crate) async fn add_widget(
    State(state): State<AppState>,
    principal: Principal,
    ws: WorkspaceContext,
    Path(dashboard_id): Path<Uuid>,
    Json(req): Json<WidgetCreateRequest>,
) -> Result<Json<ox_store::DashboardWidget>, AppError> {
    principal.require_designer()?;

    let dash = state
        .store
        .get_dashboard(dashboard_id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Dashboard"))?;
    principal.require_owner(&dash.user_id, "dashboard")?;

    let widget = ox_store::DashboardWidget {
        id: Uuid::new_v4(),
        dashboard_id,
        workspace_id: ws.workspace_id,
        title: req.title,
        widget_type: req.widget_type,
        query: req.query,
        widget_spec: req.widget_spec,
        position: req.position,
        refresh_interval_secs: req.refresh_interval_secs,
        last_result: None,
        last_refreshed: None,
        thresholds: req.thresholds,
        created_at: Utc::now(),
    };

    state
        .store
        .create_widget(&widget)
        .await
        .map_err(AppError::from)?;

    Ok(Json(widget))
}

// ---------------------------------------------------------------------------
// GET /api/dashboards/:id/widgets — list widgets
// ---------------------------------------------------------------------------

pub(crate) async fn list_widgets(
    State(state): State<AppState>,
    _principal: Principal,
    Path(dashboard_id): Path<Uuid>,
) -> Result<Json<Vec<ox_store::DashboardWidget>>, AppError> {
    let widgets = state
        .store
        .list_widgets(dashboard_id)
        .await
        .map_err(AppError::from)?;
    Ok(Json(widgets))
}

// ---------------------------------------------------------------------------
// PATCH /api/dashboards/:id/widgets/:widget_id — update widget
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub(crate) struct WidgetUpdateRequest {
    pub title: Option<String>,
    pub widget_type: Option<String>,
    pub query: Option<String>,
    pub refresh_interval_secs: Option<i32>,
    pub thresholds: Option<serde_json::Value>,
}

pub(crate) async fn update_widget(
    State(state): State<AppState>,
    principal: Principal,
    Path((dashboard_id, widget_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<WidgetUpdateRequest>,
) -> Result<axum::http::StatusCode, AppError> {
    principal.require_designer()?;

    let dash = state
        .store
        .get_dashboard(dashboard_id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Dashboard"))?;
    principal.require_owner(&dash.user_id, "dashboard")?;

    state
        .store
        .update_widget(
            widget_id,
            req.title.as_deref(),
            req.widget_type.as_deref(),
            req.query.as_deref(),
            req.refresh_interval_secs,
            req.thresholds.as_ref(),
        )
        .await
        .map_err(AppError::from)?;

    Ok(axum::http::StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// DELETE /api/dashboards/:id/widgets/:widget_id — delete widget
// ---------------------------------------------------------------------------

pub(crate) async fn delete_widget(
    State(state): State<AppState>,
    principal: Principal,
    Path((dashboard_id, widget_id)): Path<(Uuid, Uuid)>,
) -> Result<axum::http::StatusCode, AppError> {
    principal.require_designer()?;

    let dash = state
        .store
        .get_dashboard(dashboard_id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Dashboard"))?;
    principal.require_owner(&dash.user_id, "dashboard")?;

    let deleted = state
        .store
        .delete_widget(widget_id)
        .await
        .map_err(AppError::from)?;

    if deleted {
        Ok(axum::http::StatusCode::NO_CONTENT)
    } else {
        Err(AppError::not_found("Dashboard widget"))
    }
}

// ---------------------------------------------------------------------------
// POST /api/dashboards/:id/share — generate a share token
// ---------------------------------------------------------------------------

pub(crate) async fn share_dashboard(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    principal.require_designer()?;

    let dash = state
        .store
        .get_dashboard(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Dashboard"))?;
    principal.require_owner(&dash.user_id, "dashboard")?;

    // Generate a cryptographically random 32-byte token (64 hex chars)
    use std::fmt::Write;
    let mut token = String::with_capacity(64);
    for b in Uuid::new_v4()
        .into_bytes()
        .iter()
        .chain(Uuid::new_v4().into_bytes().iter())
    {
        let _ = write!(token, "{b:02x}");
    }

    state
        .store
        .update_dashboard_share_token(id, Some(&token))
        .await
        .map_err(AppError::from)?;

    Ok(Json(serde_json::json!({ "share_token": token })))
}

// ---------------------------------------------------------------------------
// DELETE /api/dashboards/:id/share — revoke share token
// ---------------------------------------------------------------------------

pub(crate) async fn unshare_dashboard(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
) -> Result<axum::http::StatusCode, AppError> {
    principal.require_designer()?;

    let dash = state
        .store
        .get_dashboard(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Dashboard"))?;
    principal.require_owner(&dash.user_id, "dashboard")?;

    state
        .store
        .update_dashboard_share_token(id, None)
        .await
        .map_err(AppError::from)?;

    Ok(axum::http::StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// GET /api/shared/dashboards/:token — public dashboard viewer (no auth)
// ---------------------------------------------------------------------------

pub(crate) async fn get_shared_dashboard(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Result<Json<SharedDashboardResponse>, AppError> {
    // Public endpoint — no auth, no workspace context.
    // Use SYSTEM_BYPASS to read through RLS; the share token itself is authorization.
    let store = state.store.clone();
    let (dashboard, widgets) = ox_store::SYSTEM_BYPASS
        .scope(true, async move {
            let dashboard = store
                .get_dashboard_by_share_token(&token)
                .await
                .map_err(AppError::from)?
                .ok_or_else(|| AppError::not_found("Shared dashboard"))?;

            let widgets = store
                .list_widgets(dashboard.id)
                .await
                .map_err(AppError::from)?;

            Ok::<_, AppError>((dashboard, widgets))
        })
        .await?;

    let safe_widgets: Vec<SharedWidgetResponse> = widgets
        .into_iter()
        .map(|w| SharedWidgetResponse {
            id: w.id,
            title: w.title,
            widget_type: w.widget_type,
            widget_spec: w.widget_spec,
            position: w.position,
            last_result: w.last_result,
            last_refreshed: w.last_refreshed,
            thresholds: w.thresholds,
        })
        .collect();

    Ok(Json(SharedDashboardResponse {
        id: dashboard.id,
        name: dashboard.name,
        description: dashboard.description,
        layout: dashboard.layout,
        widgets: safe_widgets,
    }))
}

/// Public-safe view of a shared dashboard. Excludes user_id, share_token,
/// timestamps, and other internal fields.
#[derive(serde::Serialize)]
pub(crate) struct SharedDashboardResponse {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub layout: serde_json::Value,
    pub widgets: Vec<SharedWidgetResponse>,
}

/// Public-safe widget view. Excludes workspace_id, dashboard_id, and raw query.
#[derive(serde::Serialize)]
pub(crate) struct SharedWidgetResponse {
    pub id: Uuid,
    pub title: String,
    pub widget_type: String,
    pub widget_spec: serde_json::Value,
    pub position: serde_json::Value,
    pub last_result: Option<serde_json::Value>,
    pub last_refreshed: Option<chrono::DateTime<chrono::Utc>>,
    pub thresholds: Option<serde_json::Value>,
}
