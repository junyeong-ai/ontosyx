//! Φ6 #1 — admin CRUD for the per-workspace `ChangeRoutingRule`
//! overrides. The runtime resolution path
//! (`store.resolve_change_routing(change_type)`) was already
//! DB-driven; this module adds the HTTP surface so an admin can
//! actually edit the workspace's row through the UI rather than
//! by hand-running SQL.
//!
//! Routes:
//!
//! - `GET    /api/admin/governance/routing` — list every visible
//!   row (global defaults + the current workspace's overrides).
//! - `PUT    /api/admin/governance/routing/{change_type}` — upsert
//!   a workspace override for the named `ChangeType`.
//! - `DELETE /api/admin/governance/routing/{change_type}` — drop
//!   the workspace override and revert to the global default.
//!
//! Workspace scoping: the store helpers already filter through RLS,
//! so we never name `workspace_id` explicitly here. The store fills
//! it from the task-local `app.workspace_id` the middleware sets.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use ox_ontology::change_routing::{
    ApprovalRouting, ChangeRoutingRule, ChangeType, RiskLevel,
};

use crate::error::AppError;
use crate::principal::Principal;
use crate::response::ApiResponse;
use crate::state::AppState;

/// Wire shape returned by the list / upsert endpoints. The
/// `change_type` round-trips through serde so the wire string
/// matches the storage format exactly (snake_case discriminator).
/// `routing` and `risk_level` come straight off the IR types —
/// no parallel schema, the FE consumes them through the
/// `OntologyEditOp` TS union alongside the rest of the matrix.
#[derive(Debug, Serialize, ToSchema)]
pub struct ChangeRoutingRuleResponse {
    /// `false` for global defaults, `true` for workspace overrides.
    /// The UI uses this to badge override vs default rows.
    pub workspace_scoped: bool,
    /// `ChangeType` discriminator as a snake_case string.
    #[schema(value_type = String)]
    pub change_type: serde_json::Value,
    pub routing: ApprovalRouting,
    pub risk_level: RiskLevel,
    pub priority: i32,
}

impl ChangeRoutingRuleResponse {
    fn from_rule(r: ChangeRoutingRule) -> Result<Self, AppError> {
        let change_type = serde_json::to_value(r.change_type)
            .map_err(|e| AppError::internal(format!("change_type serialise failed: {e}")))?;
        Ok(Self {
            workspace_scoped: r.workspace_id.is_some(),
            change_type,
            routing: r.routing,
            risk_level: r.risk_level,
            priority: r.priority,
        })
    }
}

#[utoipa::path(
    get,
    path = "/api/admin/governance/routing",
    tag = "Admin",
    responses(
        (status = 200, description = "Every routing rule visible to the current workspace (global defaults + workspace overrides).", body = Vec<ChangeRoutingRuleResponse>),
        (status = 403, description = "Admin role required.", body = crate::openapi::ErrorResponse),
    ),
    security(("api_key" = [])),
)]
pub(crate) async fn list_routing_rules(
    State(state): State<AppState>,
    principal: Principal,
) -> Result<Json<ApiResponse<Vec<ChangeRoutingRuleResponse>>>, AppError> {
    principal.require_admin()?;
    let rules = state
        .store
        .list_change_routing_rules()
        .await
        .map_err(AppError::from)?;
    let mut out = Vec::with_capacity(rules.len());
    for r in rules {
        out.push(ChangeRoutingRuleResponse::from_rule(r)?);
    }
    Ok(ApiResponse::of(out))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpsertRoutingRuleRequest {
    pub routing: ApprovalRouting,
    /// Risk badge surfaced in the admin UI; does not influence
    /// routing directly (that's `routing`'s job).
    #[serde(default = "default_risk_level")]
    pub risk_level: RiskLevel,
    /// Priority — workspace overrides ship with `100` so they win
    /// against the global `0`. Callers normally don't set this; we
    /// default to `100` so a freshly-edited override clearly out-
    /// ranks the seed row.
    #[serde(default = "default_workspace_priority")]
    pub priority: i32,
}

fn default_risk_level() -> RiskLevel {
    RiskLevel::Medium
}

fn default_workspace_priority() -> i32 {
    100
}

#[utoipa::path(
    put,
    path = "/api/admin/governance/routing/{change_type}",
    tag = "Admin",
    params(("change_type" = String, Path, description = "Snake-case ChangeType discriminator (e.g. `glossary_term_create`).")),
    request_body = UpsertRoutingRuleRequest,
    responses(
        (status = 200, description = "Override upserted at workspace scope.", body = ChangeRoutingRuleResponse),
        (status = 400, description = "Unknown change_type.", body = crate::openapi::ErrorResponse),
        (status = 403, description = "Admin role required.", body = crate::openapi::ErrorResponse),
    ),
    security(("api_key" = [])),
)]
pub(crate) async fn upsert_routing_rule(
    State(state): State<AppState>,
    principal: Principal,
    Path(change_type_wire): Path<String>,
    Json(req): Json<UpsertRoutingRuleRequest>,
) -> Result<Json<ApiResponse<ChangeRoutingRuleResponse>>, AppError> {
    principal.require_admin()?;
    let change_type = parse_change_type(&change_type_wire)?;
    // Build the rule with workspace_id = None so the store fills it
    // from app.workspace_id; the id is fresh because the store
    // upserts on (workspace_id, change_type), not on id.
    let rule = ChangeRoutingRule {
        id: ox_ontology::change_routing::ChangeRoutingRuleId::new(format!(
            "{}-pending",
            change_type_wire
        )),
        workspace_id: None,
        change_type,
        routing: req.routing,
        risk_level: req.risk_level,
        priority: req.priority,
        created_at: chrono::Utc::now(),
    };
    let upserted = state
        .store
        .upsert_change_routing_rule(rule)
        .await
        .map_err(AppError::from)?;
    Ok(ApiResponse::of(ChangeRoutingRuleResponse::from_rule(
        upserted,
    )?))
}

#[utoipa::path(
    delete,
    path = "/api/admin/governance/routing/{change_type}",
    tag = "Admin",
    params(("change_type" = String, Path, description = "Snake-case ChangeType discriminator.")),
    responses(
        (status = 204, description = "Workspace override removed; the global default now applies."),
        (status = 400, description = "Unknown change_type.", body = crate::openapi::ErrorResponse),
        (status = 403, description = "Admin role required.", body = crate::openapi::ErrorResponse),
        (status = 404, description = "No workspace override existed.", body = crate::openapi::ErrorResponse),
    ),
    security(("api_key" = [])),
)]
pub(crate) async fn delete_routing_rule(
    State(state): State<AppState>,
    principal: Principal,
    Path(change_type_wire): Path<String>,
) -> Result<StatusCode, AppError> {
    principal.require_admin()?;
    let change_type = parse_change_type(&change_type_wire)?;
    let removed = state
        .store
        .delete_change_routing_rule(change_type)
        .await
        .map_err(AppError::from)?;
    if removed {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::not_found(&format!(
            "workspace routing override for `{change_type_wire}`"
        )))
    }
}

/// Parse the URL path's snake_case `change_type` discriminator into
/// the typed enum. Round-trips through serde so the wire string
/// matches the storage format exactly (no parallel mapping table to
/// drift).
fn parse_change_type(wire: &str) -> Result<ChangeType, AppError> {
    serde_json::from_value(serde_json::Value::String(wire.to_string())).map_err(|_| {
        AppError::bad_request(format!(
            "unknown change_type `{wire}` — see ChangeType variants for valid wire names"
        ))
    })
}
