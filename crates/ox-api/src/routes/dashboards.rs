use axum::Json;
use axum::extract::{Path, State};
use chrono::Utc;
use serde::Deserialize;
use uuid::Uuid;

use ox_store::Dashboard;
use ox_store::store::CursorParams;

use crate::error::AppError;
use crate::principal::Principal;
use crate::response::ApiResponse;
use crate::state::AppState;
use crate::validation;
use crate::workspace::WorkspaceContext;

// ---------------------------------------------------------------------------
// POST /api/dashboards — create a new dashboard
// ---------------------------------------------------------------------------

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateDashboardRequest {
    pub name: String,
    pub description: Option<String>,
}

#[utoipa::path(
    post,
    path = "/api/dashboards",
    request_body = CreateDashboardRequest,
    responses((status = 200, description = "Dashboard created", body = Object)),
    security(("api_key" = [])),
    tag = "Dashboards",
)]
pub(crate) async fn create_dashboard(
    State(state): State<AppState>,
    principal: Principal,
    ws: WorkspaceContext,
    Json(req): Json<CreateDashboardRequest>,
) -> Result<Json<ApiResponse<Dashboard>>, AppError> {
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
        share_expires_at: None,
        layout: serde_json::json!([]),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    state
        .store
        .create_dashboard(&dashboard)
        .await
        .map_err(AppError::from)?;

    Ok(ApiResponse::of(dashboard))
}

// ---------------------------------------------------------------------------
// GET /api/dashboards — list dashboards
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/dashboards",
    params(
        ("limit" = Option<u32>, Query, description = "Max items"),
        ("cursor" = Option<String>, Query, description = "Pagination cursor"),
    ),
    responses((status = 200, description = "Caller's dashboards", body = Vec<Object>)),
    security(("api_key" = [])),
    tag = "Dashboards",
)]
pub(crate) async fn list_dashboards(
    State(state): State<AppState>,
    principal: Principal,
    axum::extract::Query(pagination): axum::extract::Query<CursorParams>,
) -> Result<Json<ApiResponse<Vec<Dashboard>>>, AppError> {
    let is_admin = principal.role.is_admin();
    let page = state
        .store
        .list_dashboards(&principal.id, is_admin, &pagination)
        .await
        .map_err(AppError::from)?;
    Ok(ApiResponse::page(page))
}

// ---------------------------------------------------------------------------
// GET /api/dashboards/:id — get a single dashboard
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/dashboards/{id}",
    params(("id" = Uuid, Path, description = "Dashboard ID")),
    responses(
        (status = 200, description = "Dashboard", body = Object),
        (status = 404, description = "Not found or not accessible"),
    ),
    security(("api_key" = [])),
    tag = "Dashboards",
)]
pub(crate) async fn get_dashboard(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<Dashboard>>, AppError> {
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

    Ok(ApiResponse::of(dashboard))
}

// ---------------------------------------------------------------------------
// PATCH /api/dashboards/:id — update dashboard
// ---------------------------------------------------------------------------

#[derive(Deserialize, utoipa::ToSchema)]
pub struct UpdateDashboardRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    #[schema(value_type = Option<Object>)]
    pub layout: Option<serde_json::Value>,
    pub is_public: Option<bool>,
}

#[utoipa::path(
    patch,
    path = "/api/dashboards/{id}",
    params(("id" = Uuid, Path, description = "Dashboard ID")),
    request_body = UpdateDashboardRequest,
    responses(
        (status = 200, description = "Dashboard updated", body = Object),
        (status = 403, description = "Not the dashboard owner"),
        (status = 404, description = "Dashboard not found"),
    ),
    security(("api_key" = [])),
    tag = "Dashboards",
)]
pub(crate) async fn update_dashboard(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateDashboardRequest>,
) -> Result<Json<ApiResponse<Dashboard>>, AppError> {
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

    Ok(ApiResponse::of(dashboard))
}

// ---------------------------------------------------------------------------
// DELETE /api/dashboards/:id — delete dashboard
// ---------------------------------------------------------------------------

#[utoipa::path(
    delete,
    path = "/api/dashboards/{id}",
    params(("id" = Uuid, Path, description = "Dashboard ID")),
    responses(
        (status = 204, description = "Dashboard deleted"),
        (status = 403, description = "Not the dashboard owner"),
        (status = 404, description = "Dashboard not found"),
    ),
    security(("api_key" = [])),
    tag = "Dashboards",
)]
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
pub struct CreateWidgetRequest {
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

#[utoipa::path(
    post,
    path = "/api/dashboards/{id}/widgets",
    params(("id" = Uuid, Path, description = "Dashboard ID")),
    request_body = CreateWidgetRequest,
    responses(
        (status = 200, description = "Widget added", body = Object),
        (status = 403, description = "Not the dashboard owner"),
        (status = 404, description = "Dashboard not found"),
    ),
    security(("api_key" = [])),
    tag = "Dashboards",
)]
pub(crate) async fn add_widget(
    State(state): State<AppState>,
    principal: Principal,
    ws: WorkspaceContext,
    Path(dashboard_id): Path<Uuid>,
    Json(req): Json<CreateWidgetRequest>,
) -> Result<Json<ApiResponse<ox_store::DashboardWidget>>, AppError> {
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

    Ok(ApiResponse::of(widget))
}

// ---------------------------------------------------------------------------
// GET /api/dashboards/:id/widgets — list widgets
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/dashboards/{id}/widgets",
    params(("id" = Uuid, Path, description = "Dashboard ID")),
    responses((status = 200, description = "Widgets on the dashboard", body = Vec<Object>)),
    security(("api_key" = [])),
    tag = "Dashboards",
)]
pub(crate) async fn list_widgets(
    State(state): State<AppState>,
    _principal: Principal,
    Path(dashboard_id): Path<Uuid>,
) -> Result<Json<ApiResponse<Vec<ox_store::DashboardWidget>>>, AppError> {
    let widgets = state
        .store
        .list_widgets(dashboard_id)
        .await
        .map_err(AppError::from)?;
    Ok(ApiResponse::of(widgets))
}

// ---------------------------------------------------------------------------
// PATCH /api/dashboards/:id/widgets/:widget_id — update widget
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct WidgetUpdateRequest {
    pub title: Option<String>,
    pub widget_type: Option<String>,
    pub query: Option<String>,
    pub refresh_interval_secs: Option<i32>,
    #[schema(value_type = Option<Object>)]
    pub thresholds: Option<serde_json::Value>,
}

#[utoipa::path(
    patch,
    path = "/api/dashboards/{id}/widgets/{widget_id}",
    params(
        ("id" = Uuid, Path, description = "Dashboard ID"),
        ("widget_id" = Uuid, Path, description = "Widget ID"),
    ),
    request_body = WidgetUpdateRequest,
    responses(
        (status = 204, description = "Widget updated"),
        (status = 403, description = "Not the dashboard owner"),
        (status = 404, description = "Dashboard or widget not found"),
    ),
    security(("api_key" = [])),
    tag = "Dashboards",
)]
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

#[utoipa::path(
    delete,
    path = "/api/dashboards/{id}/widgets/{widget_id}",
    params(
        ("id" = Uuid, Path, description = "Dashboard ID"),
        ("widget_id" = Uuid, Path, description = "Widget ID"),
    ),
    responses(
        (status = 204, description = "Widget deleted"),
        (status = 403, description = "Not the dashboard owner"),
        (status = 404, description = "Dashboard or widget not found"),
    ),
    security(("api_key" = [])),
    tag = "Dashboards",
)]
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

#[derive(serde::Deserialize, utoipa::ToSchema, Default)]
pub struct ShareDashboardRequest {
    /// Days until the token expires. Defaults to `dashboards.default_share_expiry_days`
    /// (30 unless overridden); capped at `dashboards.max_share_expiry_days` (365).
    #[serde(default)]
    pub expires_in_days: Option<u32>,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct ShareDashboardResponse {
    pub share_token: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

#[utoipa::path(
    post,
    path = "/api/dashboards/{id}/share",
    params(("id" = Uuid, Path, description = "Dashboard ID")),
    request_body = ShareDashboardRequest,
    responses(
        (status = 200, description = "Share token issued", body = ShareDashboardResponse),
        (status = 403, description = "Not the dashboard owner"),
        (status = 404, description = "Dashboard not found"),
    ),
    security(("api_key" = [])),
    tag = "Dashboards",
)]
pub(crate) async fn share_dashboard(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
    body: Option<Json<ShareDashboardRequest>>,
) -> Result<Json<ApiResponse<ShareDashboardResponse>>, AppError> {
    principal.require_designer()?;

    let dash = state
        .store
        .get_dashboard(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Dashboard"))?;
    principal.require_owner(&dash.user_id, "dashboard")?;

    let req = body.map(|Json(b)| b).unwrap_or_default();
    let dashboards_cfg = &state.dashboards;
    let days = req
        .expires_in_days
        .unwrap_or(dashboards_cfg.default_share_expiry_days)
        .min(dashboards_cfg.max_share_expiry_days)
        .max(1);
    let expires_at = Utc::now() + chrono::Duration::days(days as i64);

    // 256 bits of CSPRNG entropy (same helper as `create_api_key`),
    // not UUID-concatenation — UUIDs reserve 6 bits for version/variant.
    let token = ox_store::secret_token::generate_hex(32);

    state
        .store
        .update_dashboard_share_token(id, Some(&token), Some(expires_at))
        .await
        .map_err(AppError::from)?;

    Ok(ApiResponse::of(ShareDashboardResponse {
        share_token: token,
        expires_at,
    }))
}

// ---------------------------------------------------------------------------
// DELETE /api/dashboards/:id/share — revoke share token
// ---------------------------------------------------------------------------

#[utoipa::path(
    delete,
    path = "/api/dashboards/{id}/share",
    params(("id" = Uuid, Path, description = "Dashboard ID")),
    responses(
        (status = 204, description = "Share token revoked"),
        (status = 403, description = "Not the dashboard owner"),
        (status = 404, description = "Dashboard not found"),
    ),
    security(("api_key" = [])),
    tag = "Dashboards",
)]
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
        .update_dashboard_share_token(id, None, None)
        .await
        .map_err(AppError::from)?;

    Ok(axum::http::StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// GET /api/shared/dashboards/:token — public dashboard viewer (no auth)
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/shared/dashboards/{token}",
    params(("token" = String, Path, description = "Share token")),
    responses(
        (status = 200, description = "Public-safe dashboard view", body = SharedDashboardResponse),
        (status = 404, description = "Token not found"),
        (status = 410, description = "Token expired"),
    ),
    tag = "Dashboards",
)]
pub(crate) async fn get_shared_dashboard(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Result<Json<ApiResponse<SharedDashboardResponse>>, AppError> {
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

            // 410 Gone for expired tokens so the client can render a
            // distinct "this share link has expired" message rather than
            // a generic 404 (which would mask the difference between
            // "never existed" and "no longer valid").
            if let Some(expires_at) = dashboard.share_expires_at
                && expires_at < Utc::now()
            {
                return Err(AppError::gone("Shared dashboard link has expired"));
            }

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

    Ok(ApiResponse::of(SharedDashboardResponse {
        id: dashboard.id,
        name: dashboard.name,
        description: dashboard.description,
        layout: dashboard.layout,
        widgets: safe_widgets,
    }))
}

/// Public-safe view of a shared dashboard. Excludes user_id, share_token,
/// timestamps, and other internal fields.
#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct SharedDashboardResponse {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    #[schema(value_type = Object)]
    pub layout: serde_json::Value,
    pub widgets: Vec<SharedWidgetResponse>,
}

/// Public-safe widget view. Excludes workspace_id, dashboard_id, and raw query.
#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct SharedWidgetResponse {
    pub id: Uuid,
    pub title: String,
    pub widget_type: String,
    #[schema(value_type = Object)]
    pub widget_spec: serde_json::Value,
    #[schema(value_type = Object)]
    pub position: serde_json::Value,
    #[schema(value_type = Option<Object>)]
    pub last_result: Option<serde_json::Value>,
    pub last_refreshed: Option<chrono::DateTime<chrono::Utc>>,
    #[schema(value_type = Option<Object>)]
    pub thresholds: Option<serde_json::Value>,
}
