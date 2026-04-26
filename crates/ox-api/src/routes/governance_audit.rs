//! Workspace-wide PROV-O audit trail.
//!
//! Streams `ProvenanceDef` entries across every committed ontology
//! in the caller's workspace, filtered + cursor-paginated. The
//! page surface is at `/settings/governance/audit`.

use axum::Json;
use axum::extract::{Query, State};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use uuid::Uuid;

use ox_store::{AuditRecord, AuditTrailFilter, CursorPage};

use crate::error::AppError;
use crate::principal::Principal;
use crate::response::ApiResponse;
use crate::state::AppState;
use crate::workspace::WorkspaceContext;

const DEFAULT_LIMIT: u32 = 50;
const MAX_LIMIT: u32 = 200;

#[derive(Debug, Deserialize)]
pub struct AuditQuery {
    pub ontology_id: Option<Uuid>,
    pub activity_kind: Option<String>,
    pub agent_kind: Option<String>,
    pub since: Option<DateTime<Utc>>,
    pub until: Option<DateTime<Utc>>,
    pub cursor: Option<String>,
    pub limit: Option<u32>,
}

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
