use std::collections::HashMap;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use chrono::Utc;
use serde::Deserialize;
use tracing::{info, warn};
use uuid::Uuid;

use ox_core::types::PropertyValue;
use ox_store::{
    MetricWindow, QualityDashboardEntry, QualityMetricsReport, QualityResult, QualityRule,
    ShaclFailureCount, StaleConceptProposal, StaleProposalDecision, StaleTypeEntry,
    WorkspaceQualityBaseline,
};

use crate::error::AppError;
use crate::principal::Principal;
use crate::response::ApiResponse;
use crate::state::AppState;
use crate::workspace::WorkspaceContext;

// ---------------------------------------------------------------------------
// Request / Query types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct CreateRuleRequest {
    pub name: String,
    pub rule_type: String,
    /// Ontology lineage this rule is scoped to. Required so two ontologies
    /// in the same workspace that share a label name don't collapse each
    /// other's rules.
    pub ontology_lineage_id: String,
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
    /// Optional lineage filter — when present, only rules scoped to this
    /// ontology lineage are returned.
    pub ontology_lineage_id: Option<String>,
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
) -> Result<(StatusCode, Json<ApiResponse<QualityRule>>), AppError> {
    principal.require_designer()?;

    let rule = QualityRule {
        id: Uuid::new_v4(),
        workspace_id: ws.workspace_id,
        ontology_lineage_id: req.ontology_lineage_id,
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

    Ok((StatusCode::CREATED, ApiResponse::of(rule)))
}

// ---------------------------------------------------------------------------
// GET /api/quality/rules — list quality rules
// ---------------------------------------------------------------------------

pub(crate) async fn list_rules(
    State(state): State<AppState>,
    Query(params): Query<ListRulesParams>,
) -> Result<Json<ApiResponse<Vec<QualityRule>>>, AppError> {
    let rules = state
        .store
        .list_quality_rules(
            params.ontology_lineage_id.as_deref(),
            params.target_label.as_deref(),
        )
        .await
        .map_err(AppError::from)?;

    Ok(ApiResponse::of(rules))
}

// ---------------------------------------------------------------------------
// GET /api/quality/rules/:id — get single rule
// ---------------------------------------------------------------------------

pub(crate) async fn get_rule(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<QualityRule>>, AppError> {
    let rule = state
        .store
        .get_quality_rule(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Quality rule"))?;

    Ok(ApiResponse::of(rule))
}

// ---------------------------------------------------------------------------
// PATCH /api/quality/rules/:id — update rule
// ---------------------------------------------------------------------------

pub(crate) async fn update_rule(
    State(state): State<AppState>,
    principal: Principal,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateRuleRequest>,
) -> Result<Json<ApiResponse<QualityRule>>, AppError> {
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

    Ok(ApiResponse::of(updated))
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
) -> Result<Json<ApiResponse<Vec<QualityDashboardEntry>>>, AppError> {
    let entries = state
        .store
        .list_quality_dashboard_entries()
        .await
        .map_err(AppError::from)?;

    Ok(ApiResponse::of(entries))
}

// ---------------------------------------------------------------------------
// GET /api/quality/rules/:id/results — latest results for a rule
// ---------------------------------------------------------------------------

pub(crate) async fn rule_results(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(params): Query<LimitParam>,
) -> Result<Json<ApiResponse<Vec<QualityResult>>>, AppError> {
    let limit = params.limit.unwrap_or(20);

    let results = state
        .store
        .list_latest_results(id, limit)
        .await
        .map_err(AppError::from)?;

    Ok(ApiResponse::of(results))
}

// ---------------------------------------------------------------------------
// POST /api/quality/rules/:id/execute — execute a single quality rule
// ---------------------------------------------------------------------------

pub(crate) async fn execute_rule(
    State(state): State<AppState>,
    principal: Principal,
    ws: WorkspaceContext,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<QualityResult>>, AppError> {
    principal.require_designer()?;

    let rule = state
        .store
        .get_quality_rule(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Quality rule"))?;

    let runtime = state.runtime.as_ref().ok_or_else(AppError::no_runtime)?;

    let result = execute_single_rule(&rule, runtime.as_ref(), ws.workspace_id).await?;

    state
        .store
        .record_quality_result(&result)
        .await
        .map_err(AppError::from)?;

    // Fire-and-forget notification (spawn_scoped captures workspace context)
    let store_clone = state.store.clone();
    let ws_id = ws.workspace_id;
    let rule_name = rule.name.clone();
    let passed = result.passed;
    let actual_value = result.actual_value;
    crate::spawn_scoped::spawn_scoped(async move {
        super::notifications::dispatch_quality_notification(
            store_clone.as_ref(),
            ws_id,
            &rule_name,
            passed,
            actual_value,
        )
        .await;
    });

    info!(
        rule_id = %id,
        passed = result.passed,
        value = ?result.actual_value,
        "Quality rule executed"
    );

    Ok(ApiResponse::of(result))
}

// ---------------------------------------------------------------------------
// POST /api/quality/execute-all — execute all active quality rules
// ---------------------------------------------------------------------------

pub(crate) async fn execute_all_rules(
    State(state): State<AppState>,
    principal: Principal,
    ws: WorkspaceContext,
) -> Result<Json<ApiResponse<Vec<QualityResult>>>, AppError> {
    principal.require_designer()?;

    let runtime = state.runtime.as_ref().ok_or_else(AppError::no_runtime)?;

    let rules = state
        .store
        .list_quality_rules(None, None)
        .await
        .map_err(AppError::from)?;

    let active_rules: Vec<_> = rules.into_iter().filter(|r| r.is_active).collect();
    let mut results = Vec::with_capacity(active_rules.len());

    for rule in &active_rules {
        match execute_single_rule(rule, runtime.as_ref(), ws.workspace_id).await {
            Ok(result) => {
                if let Err(e) = state.store.record_quality_result(&result).await {
                    warn!(rule_id = %rule.id, error = %e, "Failed to record quality result");
                }
                // Fire-and-forget notification (spawn_scoped captures workspace context)
                let store_clone = state.store.clone();
                let ws_id = ws.workspace_id;
                let rule_name = rule.name.clone();
                let passed = result.passed;
                let actual_value = result.actual_value;
                crate::spawn_scoped::spawn_scoped(async move {
                    super::notifications::dispatch_quality_notification(
                        store_clone.as_ref(),
                        ws_id,
                        &rule_name,
                        passed,
                        actual_value,
                    )
                    .await;
                });
                results.push(result);
            }
            Err(e) => {
                let err_msg = format!("{e:?}");
                warn!(rule_id = %rule.id, error = %err_msg, "Quality rule execution failed");
                // Record a failed result so the dashboard reflects the error
                let failed_result = QualityResult {
                    id: Uuid::new_v4(),
                    workspace_id: ws.workspace_id,
                    rule_id: rule.id,
                    passed: false,
                    actual_value: None,
                    details: serde_json::json!({ "error": err_msg }),
                    evaluated_at: Utc::now(),
                };
                let _ = state.store.record_quality_result(&failed_result).await;
                results.push(failed_result);
            }
        }
    }

    info!(
        count = results.len(),
        passed = results.iter().filter(|r| r.passed).count(),
        "Executed all quality rules"
    );

    Ok(ApiResponse::of(results))
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Build a Cypher query for a quality rule based on its type.
///
/// All generated queries return two columns: `violations` (count of failing
/// items) and `total` (count of all checked items). This allows computing a
/// quality score as `(total - violations) / total * 100`.
fn build_quality_cypher(rule: &QualityRule) -> Result<String, AppError> {
    // Sanitize label: only allow alphanumeric + underscore (prevent injection)
    let label = &rule.target_label;
    if !label.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return Err(AppError::bad_request(format!(
            "Invalid target label: {label}"
        )));
    }

    let prop = rule.target_property.as_deref().unwrap_or("");
    if !prop.is_empty() && !prop.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return Err(AppError::bad_request(format!(
            "Invalid target property: {prop}"
        )));
    }

    match rule.rule_type.as_str() {
        "completeness" => {
            if prop.is_empty() {
                return Err(AppError::bad_request(
                    "Completeness rules require a target_property",
                ));
            }
            Ok(format!(
                "MATCH (n:`{label}`) \
                 WITH count(n) AS total, \
                 count(CASE WHEN n.`{prop}` IS NULL THEN 1 END) AS violations \
                 RETURN violations, total"
            ))
        }
        "uniqueness" => {
            if prop.is_empty() {
                return Err(AppError::bad_request(
                    "Uniqueness rules require a target_property",
                ));
            }
            Ok(format!(
                "MATCH (n:`{label}`) \
                 WITH n.`{prop}` AS val, count(*) AS cnt \
                 WITH sum(CASE WHEN cnt > 1 THEN cnt ELSE 0 END) AS violations, \
                 sum(cnt) AS total \
                 RETURN violations, total"
            ))
        }
        "freshness" => {
            // Freshness: check for nodes with no recent updates
            // If a property is given, checks that property; otherwise checks existence
            if prop.is_empty() {
                Ok(format!(
                    "OPTIONAL MATCH (n:`{label}`) \
                     RETURN CASE WHEN count(n) > 0 THEN 0 ELSE 1 END AS violations, \
                     1 AS total"
                ))
            } else {
                Ok(format!(
                    "MATCH (n:`{label}`) \
                     WITH count(n) AS total, \
                     count(CASE WHEN n.`{prop}` IS NULL THEN 1 END) AS violations \
                     RETURN violations, total"
                ))
            }
        }
        "consistency" => {
            // Consistency: check that a property is non-null when it exists
            if prop.is_empty() {
                return Err(AppError::bad_request(
                    "Consistency rules require a target_property",
                ));
            }
            Ok(format!(
                "MATCH (n:`{label}`) \
                 WITH count(n) AS total, \
                 count(CASE WHEN n.`{prop}` IS NULL THEN 1 END) AS violations \
                 RETURN violations, total"
            ))
        }
        "custom" => {
            let cypher = rule.cypher_check.as_deref().unwrap_or("").trim();
            if cypher.is_empty() {
                return Err(AppError::bad_request(
                    "Custom rules require a cypher_check query",
                ));
            }
            // Validate that the custom query is read-only
            let upper = cypher.to_uppercase();
            const WRITE_KEYWORDS: &[&str] =
                &["DELETE", "DETACH", "CREATE", "MERGE", "SET ", "REMOVE "];
            if WRITE_KEYWORDS.iter().any(|kw| upper.contains(kw)) {
                return Err(AppError::bad_request(
                    "Custom cypher_check must be read-only (no write operations)",
                ));
            }
            Ok(cypher.to_string())
        }
        other => Err(AppError::bad_request(format!("Unknown rule type: {other}"))),
    }
}

/// Extract violations/total from query result rows.
///
/// Expects the query to return columns named `violations` and `total`.
/// Falls back to first two numeric columns if names don't match.
fn parse_violations(result: &ox_query_ir::query::QueryResult) -> (i64, i64) {
    if result.rows.is_empty() {
        return (0, 0);
    }

    let row = &result.rows[0];

    // Try to find columns by name
    let violations_idx = result
        .columns
        .iter()
        .position(|c| c.eq_ignore_ascii_case("violations"));
    let total_idx = result
        .columns
        .iter()
        .position(|c| c.eq_ignore_ascii_case("total"));

    let extract_i64 = |val: &PropertyValue| -> i64 {
        match val {
            PropertyValue::Int(v) => *v,
            PropertyValue::Float(v) => *v as i64,
            _ => 0,
        }
    };

    match (violations_idx, total_idx) {
        (Some(vi), Some(ti)) => {
            let violations = row.get(vi).map(extract_i64).unwrap_or(0);
            let total = row.get(ti).map(extract_i64).unwrap_or(0);
            (violations, total)
        }
        _ => {
            // Fallback: first column = violations, second = total
            let violations = row.first().map(extract_i64).unwrap_or(0);
            let total = row.get(1).map(extract_i64).unwrap_or(0);
            (violations, total)
        }
    }
}

/// Execute a single quality rule against the graph runtime and return a result.
async fn execute_single_rule(
    rule: &QualityRule,
    runtime: &dyn ox_runtime::GraphRuntime,
    workspace_id: Uuid,
) -> Result<QualityResult, AppError> {
    let cypher = build_quality_cypher(rule)?;

    let params: HashMap<String, PropertyValue> = HashMap::new();
    let query_result = runtime.execute_query(&cypher, &params).await.map_err(|e| {
        AppError::unprocessable(format!(
            "Quality query failed for rule '{}': {e}",
            rule.name
        ))
    })?;

    let (violations, total) = parse_violations(&query_result);

    let score = if total > 0 {
        (total - violations) as f64 / total as f64 * 100.0
    } else {
        100.0
    };

    // threshold is stored as 0.0-100.0 percentage
    let passed = score >= rule.threshold;

    Ok(QualityResult {
        id: Uuid::new_v4(),
        workspace_id,
        rule_id: rule.id,
        passed,
        actual_value: Some(score),
        details: serde_json::json!({
            "cypher": cypher,
            "violations": violations,
            "total": total,
            "score": score,
        }),
        evaluated_at: Utc::now(),
    })
}

// ===========================================================================
// Six-window metrics — patent's "6 창" dashboard. Backs
// `/quality/metrics`, `/quality/shacl-failures`, `/quality/stale-types`.
// ===========================================================================

fn parse_window(s: Option<&str>) -> MetricWindow {
    match s {
        Some("30d") => MetricWindow::Last30d,
        Some("90d") => MetricWindow::Last90d,
        _ => MetricWindow::Last7d,
    }
}

#[derive(Debug, Deserialize)]
pub struct MetricsParams {
    /// `"7d"` | `"30d"` | `"90d"`. Defaults to `7d`.
    pub window: Option<String>,
}

/// `GET /api/quality/metrics?window=7d` — six-window summary.
pub(crate) async fn get_quality_metrics(
    State(state): State<AppState>,
    _principal: Principal,
    _ws: WorkspaceContext,
    Query(params): Query<MetricsParams>,
) -> Result<Json<ApiResponse<QualityMetricsReport>>, AppError> {
    let report = state
        .store
        .aggregate_quality_metrics(parse_window(params.window.as_deref()))
        .await
        .map_err(AppError::from)?;
    Ok(ApiResponse::of(report))
}

/// `GET /api/quality/shacl-failures?window=7d` — failure-kind histogram.
pub(crate) async fn list_shacl_failures(
    State(state): State<AppState>,
    _principal: Principal,
    _ws: WorkspaceContext,
    Query(params): Query<MetricsParams>,
) -> Result<Json<ApiResponse<Vec<ShaclFailureCount>>>, AppError> {
    let rows = state
        .store
        .list_shacl_failure_distribution(parse_window(params.window.as_deref()))
        .await
        .map_err(AppError::from)?;
    Ok(ApiResponse::of(rows))
}

/// `GET /api/quality/baseline` — current workspace's adaptive
/// threshold snapshot.
///
/// Returns `null` when the daily cron hasn't populated the row
/// yet (fresh workspace, or run is still pending the first tick).
/// The frontend treats `null` as "insufficient evidence" and
/// falls back to its hardcoded prior; the same fallback applies
/// when `sample_size` is below the FE-side minimum. This is Phase
/// B of the adaptive-threshold rollout: the banner consumes the
/// JSONB `thresholds` bundle at render time when present and
/// drops into the hardcoded defaults when not — no config flip,
/// no deploy.
pub(crate) async fn get_quality_baseline(
    State(state): State<AppState>,
    _principal: Principal,
    _ws: WorkspaceContext,
) -> Result<Json<ApiResponse<Option<WorkspaceQualityBaseline>>>, AppError> {
    let baseline = state
        .store
        .get_quality_baseline()
        .await
        .map_err(AppError::from)?;
    Ok(ApiResponse::of(baseline))
}

#[derive(Debug, Deserialize)]
pub struct StaleTypesParams {
    /// Staleness cutoff in days. Defaults to 180 — matches the
    /// patent matrix's "6개월 미사용" threshold.
    pub stale_after_days: Option<i64>,
}

/// `GET /api/quality/stale-types?stale_after_days=180` — types
/// whose `last_used_at` is older than the cutoff. Candidates for
/// deprecation (always HITL — the list is advisory, not an
/// auto-delete trigger).
pub(crate) async fn list_stale_types(
    State(state): State<AppState>,
    _principal: Principal,
    _ws: WorkspaceContext,
    Query(params): Query<StaleTypesParams>,
) -> Result<Json<ApiResponse<Vec<StaleTypeEntry>>>, AppError> {
    let rows = state
        .store
        .list_stale_types(params.stale_after_days.unwrap_or(180))
        .await
        .map_err(AppError::from)?;
    Ok(ApiResponse::of(rows))
}

#[derive(Debug, Deserialize)]
pub struct StaleProposalsParams {
    /// `true` (default) — only pending proposals (admin's open queue).
    /// `false` — include terminal (approved / dismissed) for history.
    #[serde(default)]
    pub include_decided: Option<bool>,
}

/// `GET /api/quality/stale-proposals` — durable proposals written by
/// the daily cron. Natural key guarantees one open row per type.
pub(crate) async fn list_stale_proposals(
    State(state): State<AppState>,
    _principal: Principal,
    _ws: WorkspaceContext,
    Query(params): Query<StaleProposalsParams>,
) -> Result<Json<ApiResponse<Vec<StaleConceptProposal>>>, AppError> {
    let pending_only = !params.include_decided.unwrap_or(false);
    let rows = state
        .store
        .list_stale_concept_proposals(pending_only)
        .await
        .map_err(AppError::from)?;
    Ok(ApiResponse::of(rows))
}

#[derive(Debug, Deserialize)]
pub struct StaleProposalDecisionRequest {
    /// `"approved"` or `"dismissed"`. `"pending"` is rejected —
    /// there's no "un-decide" op; admins clear the row instead.
    pub decision: String,
    /// Optional admin comment. Surfaces in audit log + the
    /// proposal's row detail.
    #[serde(default)]
    pub reason: Option<String>,
}

/// `PATCH /api/quality/stale-proposals/{id}` — admin decision on
/// one proposal. Returns the updated row.
pub(crate) async fn decide_stale_proposal(
    State(state): State<AppState>,
    principal: Principal,
    _ws: WorkspaceContext,
    Path(id): Path<Uuid>,
    Json(req): Json<StaleProposalDecisionRequest>,
) -> Result<Json<ApiResponse<StaleConceptProposal>>, AppError> {
    principal.require_designer()?;
    let decision = match req.decision.as_str() {
        "approved" => StaleProposalDecision::Approved,
        "dismissed" => StaleProposalDecision::Dismissed,
        other => {
            return Err(AppError::bad_request(format!(
                "decision must be \"approved\" or \"dismissed\"; got \"{other}\""
            )));
        }
    };
    let row = state
        .store
        .record_stale_proposal_decision(id, decision, principal.user_uuid().ok(), req.reason)
        .await
        .map_err(AppError::from)?;
    Ok(ApiResponse::of(row))
}
