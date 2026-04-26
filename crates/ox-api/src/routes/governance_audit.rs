//! Workspace-wide PROV-O audit trail.
//!
//! Streams `ProvenanceDef` entries across every committed ontology
//! in the caller's workspace, filtered + cursor-paginated. The
//! page surface is at `/settings/governance/audit`.

use axum::Json;
use axum::extract::{Query, State};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use utoipa::IntoParams;
use uuid::Uuid;

use ox_store::{AuditRecord, AuditTrailFilter, CursorPage};

use crate::error::AppError;
use crate::principal::Principal;
use crate::response::ApiResponse;
use crate::state::AppState;
use crate::workspace::WorkspaceContext;

const DEFAULT_LIMIT: u32 = 50;
const MAX_LIMIT: u32 = 200;

#[derive(Debug, Deserialize, IntoParams)]
pub struct AuditQuery {
    /// Restrict to one ontology.
    pub ontology_id: Option<Uuid>,
    /// `ProvenanceActivityKind` discriminator (`source_scan`,
    /// `function_eval`, `rule_validate`, ...).
    pub activity_kind: Option<String>,
    /// `AgentRef` discriminator (`user`, `service`, `llm_model`, `system`).
    pub agent_kind: Option<String>,
    /// Inclusive lower bound on `at_time`.
    pub since: Option<DateTime<Utc>>,
    /// Inclusive upper bound on `at_time`.
    pub until: Option<DateTime<Utc>>,
    /// Cursor from the previous response's `next_cursor`.
    pub cursor: Option<String>,
    /// Page size — default 50, clamped to [1, 200].
    pub limit: Option<u32>,
}

#[utoipa::path(
    get,
    path = "/api/governance/audit",
    tag = "Audit",
    params(AuditQuery),
    responses(
        (status = 200, description = "PROV-O records across every committed ontology in the workspace, newest first.", body = crate::openapi::AuditRecordPage),
    ),
    security(("api_key" = [])),
)]
pub(crate) async fn list_audit_records(
    State(state): State<AppState>,
    _principal: Principal,
    _ws: WorkspaceContext,
    Query(q): Query<AuditQuery>,
) -> Result<Json<ApiResponse<CursorPage<AuditRecord>>>, AppError> {
    let limit = q.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT) as i64;

    let filter = AuditTrailFilter {
        ontology_id: q.ontology_id,
        activity_kind: q.activity_kind,
        agent_kind: q.agent_kind,
        since: q.since,
        until: q.until,
    };

    let page = state
        .store
        .list_audit_records(filter, q.cursor.as_deref(), limit)
        .await
        .map_err(AppError::from)?;

    Ok(ApiResponse::of(page))
}
