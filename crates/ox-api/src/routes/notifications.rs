use axum::Json;
use axum::extract::{Path, Query, State};
use chrono::Utc;
use serde::Deserialize;
use tracing::warn;
use uuid::Uuid;

use ox_store::{NotificationChannel, NotificationLog};

use crate::error::AppError;
use crate::notifications::{send_webhook, validate_channel_type, validate_webhook_url};
use crate::principal::Principal;
use crate::response::ApiResponse;
use crate::state::AppState;
use crate::workspace::WorkspaceContext;

// ---------------------------------------------------------------------------
// POST /api/notifications/channels — create channel
// ---------------------------------------------------------------------------

#[derive(Deserialize, utoipa::ToSchema)]
pub struct WebhookChannelConfig {
    /// Webhook endpoint URL. HTTP(S) only; private network ranges are
    /// blocked at validation time.
    pub url: String,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateChannelRequest {
    pub name: String,
    pub channel_type: String,
    pub config: WebhookChannelConfig,
    #[serde(default)]
    pub events: Vec<String>,
}

#[utoipa::path(
    post,
    path = "/api/notifications/channels",
    request_body = CreateChannelRequest,
    responses((status = 200, description = "Channel created", body = Object)),
    security(("api_key" = [])),
    tag = "Notifications",
)]
pub(crate) async fn create_channel(
    State(state): State<AppState>,
    principal: Principal,
    ws: WorkspaceContext,
    Json(req): Json<CreateChannelRequest>,
) -> Result<Json<ApiResponse<NotificationChannel>>, AppError> {
    principal.require_admin()?;

    validate_channel_type(&req.channel_type)?;
    validate_webhook_url(&req.config.url)?;

    let channel = NotificationChannel {
        id: Uuid::new_v4(),
        workspace_id: ws.workspace_id,
        name: req.name,
        channel_type: req.channel_type,
        config: serde_json::json!({ "url": req.config.url }),
        events: req.events,
        enabled: true,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    state
        .store
        .create_notification_channel(&channel)
        .await
        .map_err(AppError::from)?;

    Ok(ApiResponse::of(channel))
}

// ---------------------------------------------------------------------------
// GET /api/notifications/channels — list channels
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/notifications/channels",
    responses((status = 200, description = "Channel list", body = Vec<Object>)),
    security(("api_key" = [])),
    tag = "Notifications",
)]
pub(crate) async fn list_channels(
    State(state): State<AppState>,
    principal: Principal,
) -> Result<Json<ApiResponse<Vec<NotificationChannel>>>, AppError> {
    principal.require_admin()?;

    let channels = state
        .store
        .list_notification_channels()
        .await
        .map_err(AppError::from)?;
    Ok(ApiResponse::of(channels))
}

// ---------------------------------------------------------------------------
// PATCH /api/notifications/channels/:id — update channel
// ---------------------------------------------------------------------------

#[derive(Deserialize, utoipa::ToSchema)]
pub struct UpdateChannelRequest {
    pub name: Option<String>,
    pub config: Option<WebhookChannelConfig>,
    pub events: Option<Vec<String>>,
    pub enabled: Option<bool>,
}

#[utoipa::path(
    patch,
    path = "/api/notifications/channels/{id}",
    params(("id" = Uuid, Path, description = "Notification channel ID")),
    request_body = UpdateChannelRequest,
    responses((status = 204, description = "Channel updated")),
    security(("api_key" = [])),
    tag = "Notifications",
)]
pub(crate) async fn update_channel(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateChannelRequest>,
) -> Result<axum::http::StatusCode, AppError> {
    principal.require_admin()?;

    if let Some(config) = &req.config {
        validate_webhook_url(&config.url)?;
    }

    let config_value = req
        .config
        .as_ref()
        .map(|c| serde_json::json!({ "url": c.url }));
    state
        .store
        .update_notification_channel(
            id,
            req.name.as_deref(),
            config_value.as_ref(),
            req.events.as_deref(),
            req.enabled,
        )
        .await
        .map_err(AppError::from)?;

    Ok(axum::http::StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// DELETE /api/notifications/channels/:id — delete channel
// ---------------------------------------------------------------------------

#[utoipa::path(
    delete,
    path = "/api/notifications/channels/{id}",
    params(("id" = Uuid, Path, description = "Notification channel ID")),
    responses(
        (status = 204, description = "Channel deleted"),
        (status = 404, description = "Channel not found"),
    ),
    security(("api_key" = [])),
    tag = "Notifications",
)]
pub(crate) async fn delete_channel(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
) -> Result<axum::http::StatusCode, AppError> {
    principal.require_admin()?;

    let deleted = state
        .store
        .delete_notification_channel(id)
        .await
        .map_err(AppError::from)?;

    if deleted {
        Ok(axum::http::StatusCode::NO_CONTENT)
    } else {
        Err(AppError::not_found("Notification channel"))
    }
}

// ---------------------------------------------------------------------------
// POST /api/notifications/channels/:id/test — send a test notification
// ---------------------------------------------------------------------------

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct TestChannelResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[utoipa::path(
    post,
    path = "/api/notifications/channels/{id}/test",
    params(("id" = Uuid, Path, description = "Notification channel ID")),
    responses(
        (status = 200, description = "Delivery attempt result", body = TestChannelResponse),
        (status = 404, description = "Channel not found"),
    ),
    security(("api_key" = [])),
    tag = "Notifications",
)]
pub(crate) async fn test_channel(
    State(state): State<AppState>,
    principal: Principal,
    ws: WorkspaceContext,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<TestChannelResponse>>, AppError> {
    principal.require_admin()?;

    let channel = state
        .store
        .get_notification_channel(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Notification channel"))?;

    let subject = "Test Notification";
    let body = "This is a test notification from Ontosyx.";
    let result = send_webhook(&channel, subject, body).await;

    let log = NotificationLog {
        id: Uuid::new_v4(),
        workspace_id: ws.workspace_id,
        channel_id: channel.id,
        event_type: "test".to_string(),
        subject: subject.to_string(),
        body: body.to_string(),
        status: if result.is_ok() {
            "sent".into()
        } else {
            "failed".into()
        },
        error: result.as_ref().err().cloned(),
        created_at: Utc::now(),
    };

    if let Err(e) = state.store.create_notification_log(&log).await {
        warn!(channel_id = %channel.id, error = %e, "Failed to record test notification log");
    }

    match result {
        Ok(()) => Ok(ApiResponse::of(TestChannelResponse {
            success: true,
            error: None,
        })),
        Err(e) => Ok(ApiResponse::of(TestChannelResponse {
            success: false,
            error: Some(e),
        })),
    }
}

// ---------------------------------------------------------------------------
// GET /api/notifications/log — recent delivery log
// ---------------------------------------------------------------------------

#[derive(Deserialize, utoipa::IntoParams)]
pub struct LogQuery {
    #[serde(default = "default_log_limit")]
    pub limit: i64,
}

fn default_log_limit() -> i64 {
    50
}

#[utoipa::path(
    get,
    path = "/api/notifications/log",
    params(LogQuery),
    responses((status = 200, description = "Delivery log entries", body = Vec<Object>)),
    security(("api_key" = [])),
    tag = "Notifications",
)]
pub(crate) async fn list_logs(
    State(state): State<AppState>,
    principal: Principal,
    Query(q): Query<LogQuery>,
) -> Result<Json<ApiResponse<Vec<NotificationLog>>>, AppError> {
    principal.require_admin()?;

    let logs = state
        .store
        .list_notification_logs(q.limit.clamp(1, 200))
        .await
        .map_err(AppError::from)?;
    Ok(ApiResponse::of(logs))
}
