use axum::Json;
use axum::extract::{Path, Query, State};
use chrono::Utc;
use serde::Deserialize;
use tracing::warn;
use uuid::Uuid;

use ox_store::{NotificationChannel, NotificationLog};

use crate::error::AppError;
use crate::principal::Principal;
use crate::state::AppState;
use crate::workspace::WorkspaceContext;

// ---------------------------------------------------------------------------
// Shared reqwest client (created once, reused across all webhook calls)
// ---------------------------------------------------------------------------

static WEBHOOK_CLIENT: std::sync::LazyLock<reqwest::Client> = std::sync::LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .expect("failed to create webhook HTTP client")
});

// ---------------------------------------------------------------------------
// POST /api/notifications/channels — create channel
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub(crate) struct CreateChannelRequest {
    pub name: String,
    pub channel_type: String,
    pub config: serde_json::Value,
    #[serde(default)]
    pub events: Vec<String>,
}

pub(crate) async fn create_channel(
    State(state): State<AppState>,
    principal: Principal,
    ws: WorkspaceContext,
    Json(req): Json<CreateChannelRequest>,
) -> Result<Json<NotificationChannel>, AppError> {
    principal.require_admin()?;

    validate_channel_type(&req.channel_type)?;
    validate_webhook_config(&req.config)?;

    let channel = NotificationChannel {
        id: Uuid::new_v4(),
        workspace_id: ws.workspace_id,
        name: req.name,
        channel_type: req.channel_type,
        config: req.config,
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

    Ok(Json(channel))
}

// ---------------------------------------------------------------------------
// GET /api/notifications/channels — list channels
// ---------------------------------------------------------------------------

pub(crate) async fn list_channels(
    State(state): State<AppState>,
    principal: Principal,
) -> Result<Json<Vec<NotificationChannel>>, AppError> {
    principal.require_admin()?;

    let channels = state
        .store
        .list_notification_channels()
        .await
        .map_err(AppError::from)?;
    Ok(Json(channels))
}

// ---------------------------------------------------------------------------
// PATCH /api/notifications/channels/:id — update channel
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub(crate) struct UpdateChannelRequest {
    pub name: Option<String>,
    pub config: Option<serde_json::Value>,
    pub events: Option<Vec<String>>,
    pub enabled: Option<bool>,
}

pub(crate) async fn update_channel(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateChannelRequest>,
) -> Result<axum::http::StatusCode, AppError> {
    principal.require_admin()?;

    if let Some(config) = &req.config {
        validate_webhook_config(config)?;
    }

    state
        .store
        .update_notification_channel(
            id,
            req.name.as_deref(),
            req.config.as_ref(),
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

pub(crate) async fn test_channel(
    State(state): State<AppState>,
    principal: Principal,
    ws: WorkspaceContext,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
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
        Ok(()) => Ok(Json(serde_json::json!({ "success": true }))),
        Err(e) => Ok(Json(serde_json::json!({ "success": false, "error": e }))),
    }
}

// ---------------------------------------------------------------------------
// GET /api/notifications/log — recent delivery log
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub(crate) struct LogQuery {
    #[serde(default = "default_log_limit")]
    pub limit: i64,
}

fn default_log_limit() -> i64 {
    50
}

pub(crate) async fn list_logs(
    State(state): State<AppState>,
    principal: Principal,
    Query(q): Query<LogQuery>,
) -> Result<Json<Vec<NotificationLog>>, AppError> {
    principal.require_admin()?;

    let logs = state
        .store
        .list_notification_logs(q.limit.clamp(1, 200))
        .await
        .map_err(AppError::from)?;
    Ok(Json(logs))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn validate_channel_type(ct: &str) -> Result<(), AppError> {
    match ct {
        "slack_webhook" | "generic_webhook" => Ok(()),
        _ => Err(AppError::bad_request(format!(
            "Unsupported channel type: {ct}. Supported: slack_webhook, generic_webhook"
        ))),
    }
}

fn validate_webhook_config(config: &serde_json::Value) -> Result<(), AppError> {
    let url = config
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::bad_request("Missing 'url' in channel config"))?;

    let parsed = reqwest::Url::parse(url)
        .map_err(|e| AppError::bad_request(format!("Invalid webhook URL: {e}")))?;

    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(AppError::bad_request("Webhook URL must use HTTP or HTTPS"));
    }

    if let Some(host) = parsed.host_str() {
        let blocked = host == "localhost"
            || host == "[::1]"
            || host.starts_with("127.")
            || host.starts_with("10.")
            || host.starts_with("192.168.")
            || host.starts_with("172.16.")
            || host.starts_with("172.17.")
            || host.starts_with("172.18.")
            || host.starts_with("172.19.")
            || host.starts_with("172.2")
            || host.starts_with("172.30.")
            || host.starts_with("172.31.")
            || host.starts_with("169.254.");
        if blocked {
            return Err(AppError::bad_request(
                "Webhook URL must not target internal networks",
            ));
        }
    }

    Ok(())
}

/// Fire-and-forget notification dispatch. Called from quality.rs after rule execution.
///
/// Queries enabled channels for the given event type, formats a message,
/// and sends to each configured webhook. Failures are logged but not
/// propagated (fire-and-forget). Caller must ensure workspace context is set
/// via `spawn_scoped` so RLS queries succeed.
pub async fn dispatch_quality_notification(
    store: &dyn ox_store::store::Store,
    workspace_id: Uuid,
    rule_name: &str,
    passed: bool,
    actual_value: Option<f64>,
) {
    let event_type = if passed {
        "quality_rule_passed"
    } else {
        "quality_rule_failed"
    };

    let channels = match store.list_channels_for_event(event_type).await {
        Ok(ch) => ch,
        Err(e) => {
            warn!(error = %e, "Failed to list notification channels");
            return;
        }
    };

    if channels.is_empty() {
        return;
    }

    let status_text = if passed { "PASSED" } else { "FAILED" };
    let subject = format!("Quality Rule {status_text}: {rule_name}");
    let body = if let Some(val) = actual_value {
        format!("Quality rule \"{rule_name}\" {status_text} (score: {val:.1}%)")
    } else {
        format!("Quality rule \"{rule_name}\" {status_text}")
    };

    for channel in &channels {
        let send_result = send_webhook(channel, &subject, &body).await;

        let log = NotificationLog {
            id: Uuid::new_v4(),
            workspace_id,
            channel_id: channel.id,
            event_type: event_type.to_string(),
            subject: subject.clone(),
            body: body.clone(),
            status: if send_result.is_ok() {
                "sent".into()
            } else {
                "failed".into()
            },
            error: send_result.err(),
            created_at: Utc::now(),
        };

        if let Err(e) = store.create_notification_log(&log).await {
            warn!(channel_id = %channel.id, error = %e, "Failed to record notification log");
        }
    }
}

/// Send a webhook notification to a channel.
/// Uses the shared static `WEBHOOK_CLIENT` for connection pooling.
async fn send_webhook(
    channel: &NotificationChannel,
    subject: &str,
    body: &str,
) -> Result<(), String> {
    let url = channel
        .config
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Missing 'url' in channel config".to_string())?;

    let payload = match channel.channel_type.as_str() {
        "slack_webhook" => {
            let text = format!("*{subject}*\n{body}");
            // Slack limits messages to ~4000 chars; truncate safely at char boundary
            let truncated = if text.len() > 3500 {
                let end = text.floor_char_boundary(3497);
                format!("{}...", &text[..end])
            } else {
                text
            };
            serde_json::json!({ "text": truncated })
        }
        _ => serde_json::json!({
            "subject": subject,
            "body": body,
            "channel": channel.name,
        }),
    };

    let mut request = WEBHOOK_CLIENT.post(url).json(&payload);

    // Apply custom headers from config (e.g. Authorization)
    if let Some(headers) = channel.config.get("headers").and_then(|v| v.as_object()) {
        for (key, val) in headers {
            if let Some(str_val) = val.as_str() {
                request = request.header(key, str_val);
            }
        }
    }

    let response = request
        .send()
        .await
        .map_err(|e| format!("Request failed: {e}"))?;

    if response.status().is_success() {
        Ok(())
    } else {
        Err(format!(
            "HTTP {}: {}",
            response.status(),
            response.status().canonical_reason().unwrap_or("Unknown")
        ))
    }
}
