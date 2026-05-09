//! Webhook notification dispatch — shared client, validation, and
//! the quality-rule fire-and-forget dispatcher.
//!
//! `routes::notifications` owns the HTTP surface; this module owns
//! the in-process plumbing so the routes/ directory stays
//! handlers-only.

use chrono::Utc;
use tracing::warn;
use uuid::Uuid;

use ox_store::{NotificationChannel, NotificationChannelType, NotificationLog};

use crate::error::AppError;

// ---------------------------------------------------------------------------
// Shared reqwest client (created once, reused across all webhook calls)
// ---------------------------------------------------------------------------

#[allow(clippy::expect_used)]
static WEBHOOK_CLIENT: std::sync::LazyLock<reqwest::Client> = std::sync::LazyLock::new(|| {
    // Startup-only: the default reqwest client builder has no fallible
    // configuration we care about (no TLS cert paths etc.), so a failure
    // here is a runtime/platform bug that warrants aborting.
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .expect("failed to create webhook HTTP client")
});

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

pub(crate) fn validate_webhook_url(url: &str) -> Result<(), AppError> {
    let parsed =
        reqwest::Url::parse(url).map_err(|_| AppError::webhook_url_invalid("parse_failed"))?;

    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(AppError::webhook_url_invalid("bad_scheme"));
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
            return Err(AppError::webhook_url_invalid("internal_network"));
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Webhook dispatch
// ---------------------------------------------------------------------------

/// Send a webhook notification to a channel. Uses the shared
/// `WEBHOOK_CLIENT` for connection pooling.
pub(crate) async fn send_webhook(
    channel: &NotificationChannel,
    subject: &str,
    body: &str,
) -> Result<(), String> {
    let url = &channel.config.url;

    let payload = match channel.channel_type {
        NotificationChannelType::SlackWebhook => {
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
        NotificationChannelType::GenericWebhook => serde_json::json!({
            "subject": subject,
            "body": body,
            "channel": channel.name,
        }),
    };

    let mut request = WEBHOOK_CLIENT.post(url).json(&payload);

    // Apply custom headers from config (e.g. Authorization)
    for (key, value) in &channel.config.headers {
        request = request.header(key, value);
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

/// Fire-and-forget notification dispatch. Called from quality.rs
/// after rule execution. Queries enabled channels for the given
/// event type, formats a message, and sends to each configured
/// webhook. Failures are logged but not propagated. Caller must
/// ensure workspace context is set via `spawn_scoped` so RLS
/// queries succeed.
pub(crate) async fn dispatch_quality_notification(
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
