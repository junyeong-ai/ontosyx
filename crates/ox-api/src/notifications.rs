//! Webhook notification dispatch — shared client, validation, and
//! the quality-rule fire-and-forget dispatcher.
//!
//! `routes::notifications` owns the HTTP surface; this module owns
//! the in-process plumbing so the routes/ directory stays
//! handlers-only.

use chrono::Utc;
use tracing::warn;
use uuid::Uuid;

use ox_store::evaluation::RetrievalLiftRegressionAlert;
use ox_store::{NotificationChannel, NotificationChannelType, NotificationLog};

use crate::error::AppError;

/// Stable event-type tag the [`NotificationChannel.events`] list
/// subscribes against for hybrid-retrieval lift regression
/// fan-out. Pinned by [`tests::event_type_string_is_stable`] so a
/// rename never silently disconnects existing channels.
pub const EVENT_TYPE_RETRIEVAL_LIFT_REGRESSION: &str = "retrieval_lift_regression";

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

// ---------------------------------------------------------------------------
// Retrieval-lift regression dispatcher
// ---------------------------------------------------------------------------

/// Fire-and-forget hybrid-retrieval lift-regression fan-out.
///
/// Called from `compare_evaluation_runs` after the report's
/// `lift_regression_alerts` is non-empty. Each enabled channel
/// subscribed to `retrieval_lift_regression` receives a
/// structured payload:
///
/// - **Slack** — Block Kit (`text` + `blocks`) so the alert
///   renders as a header + bulleted breakdown + run-id context
///   line. Slack's text-only fallback covers clients that
///   ignore blocks.
/// - **Generic** — `{ event, baseline_run_id, candidate_run_id,
///   alerts: [...] }` envelope so operators wire any HTTP
///   listener (Teams, Discord, internal alert manager).
///
/// Caller MUST run inside `WORKSPACE_ID.scope` (typically by
/// wrapping in `spawn_scoped`) so the RLS-backed channel +
/// log queries succeed.
pub(crate) async fn dispatch_retrieval_lift_regression(
    store: &dyn ox_store::store::Store,
    workspace_id: Uuid,
    baseline_run_id: Uuid,
    candidate_run_id: Uuid,
    alerts: &[RetrievalLiftRegressionAlert],
) {
    if alerts.is_empty() {
        return;
    }

    let channels = match store
        .list_channels_for_event(EVENT_TYPE_RETRIEVAL_LIFT_REGRESSION)
        .await
    {
        Ok(ch) => ch,
        Err(e) => {
            warn!(error = %e, "Failed to list notification channels for retrieval-lift regression");
            return;
        }
    };

    if channels.is_empty() {
        return;
    }

    let subject = format!(
        "Hybrid retrieval lift regression — {} cell(s)",
        alerts.len()
    );

    for channel in &channels {
        let payload =
            render_retrieval_lift_payload(channel, baseline_run_id, candidate_run_id, alerts);
        let send_result = post_payload(channel, &payload).await;

        let log = NotificationLog {
            id: Uuid::new_v4(),
            workspace_id,
            channel_id: channel.id,
            event_type: EVENT_TYPE_RETRIEVAL_LIFT_REGRESSION.to_string(),
            subject: subject.clone(),
            body: payload.to_string(),
            status: if send_result.is_ok() {
                "sent".into()
            } else {
                "failed".into()
            },
            error: send_result.err(),
            created_at: Utc::now(),
        };

        if let Err(e) = store.create_notification_log(&log).await {
            warn!(
                channel_id = %channel.id,
                error = %e,
                "Failed to record retrieval-lift notification log",
            );
        }
    }
}

/// POST a pre-rendered JSON payload to the channel webhook,
/// applying the channel's custom headers (e.g. `Authorization:
/// Bearer …`). Returns `Err` on transport failure or non-2xx
/// status — the caller persists the message verbatim into
/// `NotificationLog.error`.
async fn post_payload(
    channel: &NotificationChannel,
    payload: &serde_json::Value,
) -> Result<(), String> {
    let mut request = WEBHOOK_CLIENT.post(&channel.config.url).json(payload);
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
            response.status().canonical_reason().unwrap_or("Unknown"),
        ))
    }
}

fn render_retrieval_lift_payload(
    channel: &NotificationChannel,
    baseline_run_id: Uuid,
    candidate_run_id: Uuid,
    alerts: &[RetrievalLiftRegressionAlert],
) -> serde_json::Value {
    match channel.channel_type {
        NotificationChannelType::SlackWebhook => {
            render_retrieval_lift_slack(baseline_run_id, candidate_run_id, alerts)
        }
        NotificationChannelType::GenericWebhook => {
            render_retrieval_lift_generic(baseline_run_id, candidate_run_id, alerts)
        }
    }
}

fn render_retrieval_lift_slack(
    baseline_run_id: Uuid,
    candidate_run_id: Uuid,
    alerts: &[RetrievalLiftRegressionAlert],
) -> serde_json::Value {
    let summary = format!(
        ":rotating_light: Hybrid retrieval lift regression — {} cell(s)",
        alerts.len(),
    );
    let mut bullets = String::with_capacity(alerts.len() * 96);
    for a in alerts {
        bullets.push_str(&format!(
            "• `{}` · `{}` — Δ {:+.3} (threshold {:.3}, n={})\n",
            a.surface.as_str(),
            a.axis.as_str(),
            a.lift_delta,
            a.threshold,
            a.candidate_paired_case_count,
        ));
    }
    let runs_line = format!(
        "_baseline `{baseline_run_id}` → candidate `{candidate_run_id}`_"
    );
    serde_json::json!({
        "text": summary,
        "blocks": [
            { "type": "section", "text": { "type": "mrkdwn", "text": summary } },
            { "type": "section", "text": { "type": "mrkdwn", "text": bullets } },
            { "type": "context", "elements": [
                { "type": "mrkdwn", "text": runs_line }
            ]}
        ],
    })
}

fn render_retrieval_lift_generic(
    baseline_run_id: Uuid,
    candidate_run_id: Uuid,
    alerts: &[RetrievalLiftRegressionAlert],
) -> serde_json::Value {
    let alert_objs: Vec<serde_json::Value> = alerts
        .iter()
        .map(|a| {
            serde_json::json!({
                "surface": a.surface.as_str(),
                "axis": a.axis.as_str(),
                "lift_delta": a.lift_delta,
                "baseline_lift": a.baseline_lift,
                "candidate_lift": a.candidate_lift,
                "threshold": a.threshold,
                "candidate_paired_case_count": a.candidate_paired_case_count,
            })
        })
        .collect();
    serde_json::json!({
        "event": EVENT_TYPE_RETRIEVAL_LIFT_REGRESSION,
        "baseline_run_id": baseline_run_id,
        "candidate_run_id": candidate_run_id,
        "alerts": alert_objs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ox_store::evaluation::{RetrievalAxis, RetrievalSurface};

    fn sample_alert() -> RetrievalLiftRegressionAlert {
        RetrievalLiftRegressionAlert {
            surface: RetrievalSurface::VerifiedQuery,
            axis: RetrievalAxis::RecallAtK,
            lift_delta: -0.08,
            baseline_lift: 0.18,
            candidate_lift: 0.10,
            threshold: -0.05,
            candidate_paired_case_count: 12,
        }
    }

    #[test]
    fn event_type_string_is_stable() {
        // Renaming this constant silently disconnects every existing
        // channel subscription. Pinned here so a future PR has to
        // update the test alongside the rename — the diff makes the
        // breaking change visible.
        assert_eq!(
            EVENT_TYPE_RETRIEVAL_LIFT_REGRESSION,
            "retrieval_lift_regression"
        );
    }

    #[test]
    fn slack_payload_carries_text_and_blocks() {
        let v = render_retrieval_lift_slack(Uuid::new_v4(), Uuid::new_v4(), &[sample_alert()]);
        let text = v["text"].as_str().expect("Slack payload must carry `text` for fallback");
        assert!(v.get("blocks").is_some(), "Slack payload must carry `blocks`");
        assert!(text.contains("Hybrid retrieval lift regression"));
        assert!(text.contains("1 cell"));
    }

    #[test]
    fn slack_payload_emits_one_bullet_per_alert() {
        let alerts = vec![sample_alert(), sample_alert(), sample_alert()];
        let v = render_retrieval_lift_slack(Uuid::new_v4(), Uuid::new_v4(), &alerts);
        let body = v["blocks"][1]["text"]["text"]
            .as_str()
            .expect("Slack section text must be a string");
        assert_eq!(body.matches('\n').count(), 3);
    }

    #[test]
    fn generic_payload_emits_typed_envelope() {
        let baseline = Uuid::new_v4();
        let candidate = Uuid::new_v4();
        let v = render_retrieval_lift_generic(baseline, candidate, &[sample_alert()]);
        assert_eq!(v["event"], "retrieval_lift_regression");
        assert_eq!(v["baseline_run_id"], baseline.to_string());
        assert_eq!(v["candidate_run_id"], candidate.to_string());
        assert_eq!(v["alerts"][0]["surface"], "verified_query");
        assert_eq!(v["alerts"][0]["axis"], "recall_at_k");
        assert_eq!(v["alerts"][0]["lift_delta"], -0.08);
    }

    #[test]
    fn render_dispatches_on_channel_type() {
        let alerts = vec![sample_alert()];
        let baseline = Uuid::new_v4();
        let candidate = Uuid::new_v4();

        let now = Utc::now();
        let mk_channel = |kind: NotificationChannelType| NotificationChannel {
            id: Uuid::new_v4(),
            workspace_id: Uuid::new_v4(),
            name: "test".to_string(),
            channel_type: kind,
            config: ox_store::WebhookNotificationConfig {
                url: "https://example.invalid/hook".to_string(),
                headers: Default::default(),
            },
            events: vec![EVENT_TYPE_RETRIEVAL_LIFT_REGRESSION.to_string()],
            enabled: true,
            created_at: now,
            updated_at: now,
        };

        let slack_payload = render_retrieval_lift_payload(
            &mk_channel(NotificationChannelType::SlackWebhook),
            baseline,
            candidate,
            &alerts,
        );
        let generic_payload = render_retrieval_lift_payload(
            &mk_channel(NotificationChannelType::GenericWebhook),
            baseline,
            candidate,
            &alerts,
        );

        // Slack carries `blocks`; generic does not.
        assert!(slack_payload.get("blocks").is_some());
        assert!(generic_payload.get("blocks").is_none());
        // Generic carries `event`; Slack does not.
        assert!(generic_payload.get("event").is_some());
        assert!(slack_payload.get("event").is_none());
    }
}
