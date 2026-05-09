//! Webhook notification dispatch — shared client, validation,
//! and the per-event payload renderers.
//!
//! `routes::notifications` owns the HTTP surface; this module
//! owns the in-process plumbing so the routes/ directory stays
//! handlers-only.
//!
//! ## Architecture
//!
//! Every notification event is a value type that implements
//! [`EventPayload`]. A single generic [`dispatch_event`] drives
//! the fan-out — list subscribed channels, render the payload
//! per channel type, POST through the shared
//! [`WEBHOOK_CLIENT`], and persist a [`NotificationLog`] row
//! with the verbatim payload + delivery status. Adding a new
//! event = a new payload struct + an [`EventPayload`] impl;
//! the fan-out machinery does not change.
//!
//! Channel-typed rendering decisions (Slack Block Kit vs
//! generic JSON envelope) live on each payload's [`render`]
//! method, not in the dispatcher — Slack-incoming-webhook
//! limits (3000-char-per-section) are clamped through the
//! shared [`clamp_slack_text`] helper so every event respects
//! the same upstream constraint.

use std::time::Duration;

use chrono::{DateTime, Utc};
use serde_json::json;
use tracing::warn;
use uuid::Uuid;

use ox_store::evaluation::RetrievalLiftRegressionAlert;
use ox_store::{
    NotificationChannel, NotificationChannelType, NotificationEventType, NotificationLog,
    NotificationLogEventType, NotificationLogStatus,
};

use crate::error::AppError;

// ---------------------------------------------------------------------------
// Shared reqwest client (created once, reused across all webhook calls)
// ---------------------------------------------------------------------------

#[allow(clippy::expect_used)]
static WEBHOOK_CLIENT: std::sync::LazyLock<reqwest::Client> = std::sync::LazyLock::new(|| {
    // Startup-only: the default reqwest client builder has no
    // fallible configuration we care about (no TLS cert paths
    // etc.), so a failure here is a runtime/platform bug that
    // warrants aborting.
    reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .expect("failed to create webhook HTTP client")
});

/// Slack incoming-webhook section blocks cap each block's
/// `text.text` at 3000 chars. We clamp at 2900 to keep
/// headroom for downstream tweaks; the truncation routine
/// reserves three more bytes for the `…` marker so the
/// returned length is `≤ SLACK_SECTION_TEXT_LIMIT` even after
/// the marker is appended.
const SLACK_SECTION_TEXT_LIMIT: usize = 2900;

/// Byte length of the ellipsis sentinel — computed at
/// compile time so the truncation budget below can never
/// drift from the actual marker.
const TRUNCATION_MARKER: char = '…';
const TRUNCATION_MARKER_BYTES: usize = TRUNCATION_MARKER.len_utf8();

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/// SSRF guard. Rejects loopback / link-local / RFC1918
/// private-range hosts so an admin who pastes an internal
/// URL into the channel form can't pivot the platform into
/// the private network. Matching uses [`IpAddr`] parsing —
/// prefix-string heuristics are unsafe (`172.2.x.x` is public
/// but matches the prefix `172.2.`; `172.16.0.0/12` covers
/// `172.16.0.0..=172.31.255.255` exactly).
///
/// Hostname literals (e.g. `localhost`, `host.docker.internal`,
/// container names) are rejected via a small explicit
/// denylist — the platform is webhook-out only, so it never
/// has a legitimate reason to dial a non-DNS-resolvable host
/// inside the operator's runtime.
pub(crate) fn validate_webhook_url(url: &str) -> Result<(), AppError> {
    let parsed =
        reqwest::Url::parse(url).map_err(|_| AppError::webhook_url_invalid("parse_failed"))?;

    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(AppError::webhook_url_invalid("bad_scheme"));
    }

    if let Some(host) = parsed.host()
        && is_internal_host(&host)
    {
        return Err(AppError::webhook_url_invalid("internal_network"));
    }

    Ok(())
}

/// Hostname denylist for the SSRF guard. Every entry is a
/// non-DNS-resolvable name that resolves to an internal
/// address inside an operator's runtime. ASCII lowercase
/// comparison so capitalisation can't bypass the gate.
const INTERNAL_HOSTNAMES: &[&str] = &[
    "localhost",
    "ip6-localhost",
    "ip6-loopback",
    "host.docker.internal",
    "gateway.docker.internal",
    "kubernetes.default",
    "kubernetes.default.svc",
];

fn is_internal_host(host: &url::Host<&str>) -> bool {
    match host {
        url::Host::Domain(name) => {
            let lower = name.to_ascii_lowercase();
            INTERNAL_HOSTNAMES.iter().any(|&n| n == lower)
        }
        url::Host::Ipv4(addr) => {
            addr.is_loopback() || addr.is_private() || addr.is_link_local() || addr.is_unspecified()
        }
        url::Host::Ipv6(addr) => {
            // `is_unique_local` is unstable on Ipv6Addr in stable
            // rust; the fc00::/7 prefix check is the canonical
            // RFC 4193 partition.
            addr.is_loopback() || addr.is_unspecified() || (addr.segments()[0] & 0xfe00) == 0xfc00
        }
    }
}

// ---------------------------------------------------------------------------
// Diagnostic delivery (admin "test channel" endpoint)
// ---------------------------------------------------------------------------

/// One-shot delivery used by `POST
/// /api/notifications/channels/{id}/test`. The test endpoint
/// is *not* an event — it bypasses the subscription fan-out
/// and posts a single message to the channel that was just
/// configured. Lives here (not in routes/) so the
/// channel-typed payload shape stays next to its peer
/// renderers and the shared transport.
///
/// The corresponding [`NotificationLog`] row is recorded with
/// [`NotificationLogEventType::Test`] so dashboards can filter
/// diagnostic deliveries out of the operational view.
pub(crate) async fn send_test_message(
    channel: &NotificationChannel,
    subject: &str,
    body: &str,
) -> Result<(), String> {
    let payload = match channel.channel_type {
        NotificationChannelType::SlackWebhook => json!({
            "text": clamp_slack_text(format!("*{subject}*\n{body}")),
        }),
        NotificationChannelType::GenericWebhook => json!({
            "subject": subject,
            "body": body,
            "channel": channel.name,
            "at": Utc::now().to_rfc3339(),
        }),
    };
    send_payload(channel, &payload).await
}

// ---------------------------------------------------------------------------
// EventPayload trait + generic fan-out
// ---------------------------------------------------------------------------

/// Polymorphic event payload. Each variant of the platform's
/// notification surface (quality-rule transition, retrieval-lift
/// regression, …) is a value type that decides three things:
///
/// 1. Which subscription tag to fan out under
///    ([`event_type`](Self::event_type)).
/// 2. The human-readable subject persisted on the
///    [`NotificationLog`] row ([`subject`](Self::subject)).
/// 3. The wire payload per channel type
///    ([`render`](Self::render)) — Slack Block Kit vs generic
///    JSON envelope.
///
/// The trait is `pub(crate)` because the only legitimate
/// callers are the route handlers in this crate that produce
/// the typed payload. External crates have no reason to
/// fabricate a notification event.
pub(crate) trait EventPayload {
    fn event_type(&self) -> NotificationEventType;
    fn subject(&self) -> String;
    fn render(&self, channel_type: NotificationChannelType) -> serde_json::Value;
}

/// Truncate a Slack section body at the closest char boundary
/// below [`SLACK_SECTION_TEXT_LIMIT`] and append the
/// [`TRUNCATION_MARKER`] so the truncation is visible to the
/// operator. Slack rejects section blocks whose `text.text`
/// exceeds 3000 chars — every event renderer feeds bullet
/// bodies through this helper so large-cardinality alarms stay
/// deliverable. The byte budget reserves
/// [`TRUNCATION_MARKER_BYTES`] up front so the returned string
/// is always `≤ SLACK_SECTION_TEXT_LIMIT` after the marker is
/// appended.
fn clamp_slack_text(s: String) -> String {
    if s.len() <= SLACK_SECTION_TEXT_LIMIT {
        return s;
    }
    let budget = SLACK_SECTION_TEXT_LIMIT - TRUNCATION_MARKER_BYTES;
    let end = s.floor_char_boundary(budget);
    let mut out = s;
    out.truncate(end);
    out.push(TRUNCATION_MARKER);
    out
}

/// Single transport. POST a pre-rendered JSON payload to the
/// channel webhook, applying the channel's custom headers
/// (e.g. `Authorization: Bearer …`). Returns `Err` on
/// transport failure or non-2xx — the caller persists the
/// message verbatim into [`NotificationLog::error`].
async fn send_payload(
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

/// Generic fan-out — list channels subscribed to
/// `payload.event_type()`, render once per channel, POST, and
/// persist a [`NotificationLog`] row whether the delivery
/// succeeded or failed. The caller MUST run inside
/// `WORKSPACE_ID.scope` (typically through `spawn_scoped`) so
/// the RLS-backed channel + log queries succeed.
pub(crate) async fn dispatch_event<P: EventPayload>(
    store: &dyn ox_store::store::Store,
    workspace_id: Uuid,
    payload: &P,
) {
    let event_type = payload.event_type();
    let log_event_type = NotificationLogEventType::from_subscription(event_type);
    let channels = match store.list_channels_for_event(event_type).await {
        Ok(ch) => ch,
        Err(e) => {
            warn!(
                event_type = event_type.as_str(),
                error = %e,
                "Failed to list notification channels",
            );
            return;
        }
    };

    if channels.is_empty() {
        return;
    }

    let subject = payload.subject();

    for channel in &channels {
        let body = payload.render(channel.channel_type);
        let send_result = send_payload(channel, &body).await;

        let (status, error) = match send_result {
            Ok(()) => (NotificationLogStatus::Sent, None),
            Err(msg) => (NotificationLogStatus::Failed, Some(msg)),
        };

        let log = NotificationLog {
            id: Uuid::new_v4(),
            workspace_id,
            channel_id: channel.id,
            event_type: log_event_type,
            subject: subject.clone(),
            body: body.to_string(),
            status,
            error,
            created_at: Utc::now(),
        };

        if let Err(e) = store.create_notification_log(&log).await {
            warn!(
                channel_id = %channel.id,
                event_type = event_type.as_str(),
                error = %e,
                "Failed to record notification log",
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Quality-rule payload
// ---------------------------------------------------------------------------

pub(crate) struct QualityRulePayload {
    rule_name: String,
    passed: bool,
    actual_value: Option<f64>,
    at: DateTime<Utc>,
}

impl QualityRulePayload {
    pub(crate) fn new(rule_name: String, passed: bool, actual_value: Option<f64>) -> Self {
        Self {
            rule_name,
            passed,
            actual_value,
            at: Utc::now(),
        }
    }

    fn status_text(&self) -> &'static str {
        if self.passed { "PASSED" } else { "FAILED" }
    }
}

impl EventPayload for QualityRulePayload {
    fn event_type(&self) -> NotificationEventType {
        if self.passed {
            NotificationEventType::QualityRulePassed
        } else {
            NotificationEventType::QualityRuleFailed
        }
    }

    fn subject(&self) -> String {
        format!(
            "Quality Rule {status}: {rule}",
            status = self.status_text(),
            rule = self.rule_name,
        )
    }

    fn render(&self, channel_type: NotificationChannelType) -> serde_json::Value {
        let body = match self.actual_value {
            Some(val) => format!(
                "Quality rule \"{name}\" {status} (score: {val:.1}%)",
                name = self.rule_name,
                status = self.status_text(),
            ),
            None => format!(
                "Quality rule \"{name}\" {status}",
                name = self.rule_name,
                status = self.status_text(),
            ),
        };
        match channel_type {
            NotificationChannelType::SlackWebhook => json!({
                "text": clamp_slack_text(format!("*{}*\n{}", self.subject(), body)),
            }),
            NotificationChannelType::GenericWebhook => json!({
                "event": self.event_type().as_str(),
                "rule_name": self.rule_name,
                "passed": self.passed,
                "actual_value": self.actual_value,
                "at": self.at.to_rfc3339(),
            }),
        }
    }
}

// ---------------------------------------------------------------------------
// Retrieval-lift regression payload
// ---------------------------------------------------------------------------

pub(crate) struct RetrievalLiftRegressionPayload {
    baseline_run_id: Uuid,
    candidate_run_id: Uuid,
    alerts: Vec<RetrievalLiftRegressionAlert>,
    at: DateTime<Utc>,
}

impl RetrievalLiftRegressionPayload {
    pub(crate) fn new(
        baseline_run_id: Uuid,
        candidate_run_id: Uuid,
        alerts: Vec<RetrievalLiftRegressionAlert>,
    ) -> Self {
        Self {
            baseline_run_id,
            candidate_run_id,
            alerts,
            at: Utc::now(),
        }
    }
}

impl EventPayload for RetrievalLiftRegressionPayload {
    fn event_type(&self) -> NotificationEventType {
        NotificationEventType::RetrievalLiftRegression
    }

    fn subject(&self) -> String {
        format!(
            "Hybrid retrieval lift regression — {} cell(s)",
            self.alerts.len(),
        )
    }

    fn render(&self, channel_type: NotificationChannelType) -> serde_json::Value {
        match channel_type {
            NotificationChannelType::SlackWebhook => {
                let summary = format!(
                    ":rotating_light: Hybrid retrieval lift regression — {} cell(s)",
                    self.alerts.len(),
                );
                let mut bullets = String::with_capacity(self.alerts.len() * 96);
                for a in &self.alerts {
                    bullets.push_str(&format!(
                        "• `{}` · `{}` — Δ {:+.3} (threshold {:.3}, n={})\n",
                        a.surface.as_str(),
                        a.axis.as_str(),
                        a.lift_delta,
                        a.threshold,
                        a.candidate_paired_case_count,
                    ));
                }
                let bullets = clamp_slack_text(bullets);
                let runs_line = format!(
                    "_baseline `{}` → candidate `{}` · {}_",
                    self.baseline_run_id,
                    self.candidate_run_id,
                    self.at.to_rfc3339(),
                );
                json!({
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
            NotificationChannelType::GenericWebhook => {
                let alert_objs: Vec<serde_json::Value> = self
                    .alerts
                    .iter()
                    .map(|a| {
                        json!({
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
                json!({
                    "event": self.event_type().as_str(),
                    "baseline_run_id": self.baseline_run_id,
                    "candidate_run_id": self.candidate_run_id,
                    "at": self.at.to_rfc3339(),
                    "alerts": alert_objs,
                })
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Public dispatch entry-points (thin wrappers over `dispatch_event`)
// ---------------------------------------------------------------------------

pub(crate) async fn dispatch_quality_notification(
    store: &dyn ox_store::store::Store,
    workspace_id: Uuid,
    rule_name: &str,
    passed: bool,
    actual_value: Option<f64>,
) {
    let payload = QualityRulePayload::new(rule_name.to_string(), passed, actual_value);
    dispatch_event(store, workspace_id, &payload).await;
}

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
    let payload =
        RetrievalLiftRegressionPayload::new(baseline_run_id, candidate_run_id, alerts.to_vec());
    dispatch_event(store, workspace_id, &payload).await;
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

    fn lift_payload(n: usize) -> RetrievalLiftRegressionPayload {
        RetrievalLiftRegressionPayload::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            std::iter::repeat_with(sample_alert).take(n).collect(),
        )
    }

    // -- shared helpers --

    #[test]
    fn clamp_slack_text_passes_short_input_through() {
        let s = "short text".to_string();
        assert_eq!(clamp_slack_text(s.clone()), s);
    }

    #[test]
    fn clamp_slack_text_truncates_with_marker_and_respects_char_boundary() {
        // 4000 ASCII chars → exceeds 2900 → must truncate +
        // append the … marker. Final length is bounded by
        // SLACK_SECTION_TEXT_LIMIT.
        let big = "x".repeat(4000);
        let out = clamp_slack_text(big);
        assert!(out.ends_with('…'));
        assert!(out.chars().count() <= SLACK_SECTION_TEXT_LIMIT);
        // No mid-codepoint slicing — Korean codepoints (3-byte
        // UTF-8) repeated past the limit must still produce
        // valid UTF-8.
        let mixed = "한".repeat(2000); // 6000 bytes
        let out = clamp_slack_text(mixed);
        // Just-must-be-valid-UTF-8 — `String::truncate` panics
        // mid-codepoint, so this asserts `floor_char_boundary`
        // saved us. The clamp also leaves headroom for `…`.
        assert!(out.ends_with('…'));
        assert!(out.len() <= SLACK_SECTION_TEXT_LIMIT);
    }

    // -- QualityRulePayload --

    #[test]
    fn quality_payload_event_type_branches_on_passed_flag() {
        let pass = QualityRulePayload::new("r".into(), true, None);
        let fail = QualityRulePayload::new("r".into(), false, None);
        assert_eq!(pass.event_type(), NotificationEventType::QualityRulePassed);
        assert_eq!(fail.event_type(), NotificationEventType::QualityRuleFailed);
    }

    #[test]
    fn quality_payload_slack_renders_bold_subject_above_body() {
        let p = QualityRulePayload::new("nullability".into(), false, Some(72.5));
        let v = p.render(NotificationChannelType::SlackWebhook);
        let text = v["text"].as_str().expect("Slack payload requires text");
        assert!(text.contains("*Quality Rule FAILED: nullability*"));
        assert!(text.contains("score: 72.5%"));
    }

    #[test]
    fn quality_payload_generic_carries_typed_envelope_with_timestamp() {
        let p = QualityRulePayload::new("nullability".into(), true, Some(99.0));
        let v = p.render(NotificationChannelType::GenericWebhook);
        assert_eq!(v["event"], "quality_rule_passed");
        assert_eq!(v["rule_name"], "nullability");
        assert_eq!(v["passed"], true);
        assert!(v.get("at").and_then(|x| x.as_str()).is_some());
    }

    // -- RetrievalLiftRegressionPayload --

    #[test]
    fn retrieval_lift_payload_event_type_is_pinned() {
        // Renaming the wire string silently disconnects every
        // existing channel subscription. The fix is the
        // ox-store enum (`NotificationEventType`) +
        // `every_variant_has_unique_wire_str` test there;
        // here we pin the event-type the payload reports.
        let p = lift_payload(1);
        assert_eq!(p.event_type(), NotificationEventType::RetrievalLiftRegression);
        assert_eq!(p.event_type().as_str(), "retrieval_lift_regression");
    }

    #[test]
    fn retrieval_lift_slack_carries_text_blocks_and_one_bullet_per_alert() {
        let p = lift_payload(3);
        let v = p.render(NotificationChannelType::SlackWebhook);
        let text = v["text"].as_str().expect("Slack payload requires text");
        assert!(text.contains("Hybrid retrieval lift regression"));
        assert!(text.contains("3 cell"));
        let body = v["blocks"][1]["text"]["text"]
            .as_str()
            .expect("Slack section text must be a string");
        assert_eq!(body.matches('\n').count(), 3);
    }

    #[test]
    fn retrieval_lift_slack_clamps_large_alarm_bodies() {
        // 200 alerts × ~96 chars ≈ 19 200 chars — well above
        // Slack's 3000-char section limit. The bullet body
        // must be clamped + marker-suffixed; otherwise Slack
        // rejects the payload and the operator never sees the
        // alarm.
        let p = lift_payload(200);
        let v = p.render(NotificationChannelType::SlackWebhook);
        let body = v["blocks"][1]["text"]["text"].as_str().unwrap();
        assert!(body.len() <= SLACK_SECTION_TEXT_LIMIT);
        assert!(body.ends_with('…'));
    }

    #[test]
    fn retrieval_lift_generic_emits_typed_envelope_with_timestamp() {
        let p = lift_payload(1);
        let v = p.render(NotificationChannelType::GenericWebhook);
        assert_eq!(v["event"], "retrieval_lift_regression");
        assert!(v.get("at").and_then(|x| x.as_str()).is_some());
        assert_eq!(v["alerts"][0]["surface"], "verified_query");
        assert_eq!(v["alerts"][0]["axis"], "recall_at_k");
        assert_eq!(v["alerts"][0]["lift_delta"], -0.08);
    }

    // -- SSRF guard --

    #[test]
    fn ssrf_guard_accepts_typical_public_webhooks() {
        for url in [
            "https://hooks.slack.com/services/T0/B0/abcdef",
            "https://example.com/webhook",
            "http://203.0.113.5/webhook",
        ] {
            assert!(
                validate_webhook_url(url).is_ok(),
                "public URL {url} must pass the SSRF guard",
            );
        }
    }

    #[test]
    fn ssrf_guard_rejects_loopback_and_link_local() {
        for url in [
            "http://localhost/webhook",
            "http://LOCALHOST/webhook",
            "http://127.0.0.1/webhook",
            "http://127.255.0.1/webhook",
            "http://[::1]/webhook",
            "http://169.254.169.254/latest/meta-data",
            "http://host.docker.internal/webhook",
        ] {
            assert!(
                validate_webhook_url(url).is_err(),
                "internal URL {url} must be blocked by the SSRF guard",
            );
        }
    }

    #[test]
    fn ssrf_guard_rejects_rfc1918_ranges_exactly() {
        // Every RFC1918 boundary — and one *just outside* —
        // exercised so a future regression of the gate
        // (back to prefix-string heuristics) fails fast.
        for url in [
            "http://10.0.0.1/",
            "http://10.255.255.254/",
            "http://192.168.0.1/",
            "http://192.168.255.254/",
            "http://172.16.0.1/",
            "http://172.20.0.1/",
            "http://172.31.255.254/",
        ] {
            assert!(
                validate_webhook_url(url).is_err(),
                "RFC1918 URL {url} must be blocked",
            );
        }
        // Just outside RFC1918 — must NOT be blocked.
        for url in ["http://172.32.0.1/", "http://172.15.0.1/", "http://172.2.0.1/"] {
            assert!(
                validate_webhook_url(url).is_ok(),
                "public IP {url} must pass — false-positive of the prefix-string gate",
            );
        }
    }

    #[test]
    fn ssrf_guard_rejects_ipv6_unique_local() {
        for url in [
            "http://[fc00::1]/", // ULA
            "http://[fd12:3456:7890::1]/",
            "http://[::]/", // unspecified
        ] {
            assert!(
                validate_webhook_url(url).is_err(),
                "IPv6 internal URL {url} must be blocked",
            );
        }
    }

    #[test]
    fn ssrf_guard_rejects_non_http_schemes() {
        for url in ["ftp://example.com/", "file:///etc/passwd", "gopher://example.com/"] {
            let err = validate_webhook_url(url).expect_err("scheme must be rejected");
            assert!(
                format!("{err:?}").contains("bad_scheme"),
                "expected bad_scheme reason for {url}, got {err:?}",
            );
        }
    }

    #[test]
    fn render_dispatches_on_channel_type() {
        let p = lift_payload(1);
        let slack = p.render(NotificationChannelType::SlackWebhook);
        let generic = p.render(NotificationChannelType::GenericWebhook);
        // Slack carries `blocks`; generic does not.
        assert!(slack.get("blocks").is_some());
        assert!(generic.get("blocks").is_none());
        // Generic carries `event`; Slack does not.
        assert!(generic.get("event").is_some());
        assert!(slack.get("event").is_none());
    }
}
