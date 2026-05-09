use axum::Json;
use axum::extract::{Query, State};
use serde::Deserialize;

use ox_store::AuditEntry;
use ox_store::store::CursorParams;

use crate::error::AppError;
use crate::principal::Principal;
use crate::response::ApiResponse;
use crate::state::AppState;

// ---------------------------------------------------------------------------
// GET /api/audit — list audit events (cursor-paginated)
//
// Admin-only. The audit log records every user action across the
// workspace; exposing it to designers or viewers would leak "who
// looked at what" signal that operators of the platform rely on
// as a governance artefact. Keep this strictly behind
// `require_admin` even though individual rows are otherwise
// workspace-scoped by RLS.
// ---------------------------------------------------------------------------

/// Mirrors `ox_store::store::CursorParams` for OpenAPI emission. The
/// store crate stays utoipa-free; the route layer owns the
/// documented surface and converts inward at the handler.
#[derive(Deserialize, utoipa::IntoParams)]
pub struct AuditCursorQuery {
    #[serde(default = "default_limit")]
    pub limit: u32,
    pub cursor: Option<String>,
}

fn default_limit() -> u32 {
    50
}

impl From<AuditCursorQuery> for CursorParams {
    fn from(q: AuditCursorQuery) -> Self {
        Self {
            limit: q.limit,
            cursor: q.cursor,
        }
    }
}

#[utoipa::path(
    get,
    path = "/api/audit",
    params(AuditCursorQuery),
    responses((status = 200, description = "Audit log entries", body = crate::openapi::AuditEntryPage)),
    security(("api_key" = [])),
    tag = "Audit",
)]
pub(crate) async fn list_audit_events(
    State(state): State<AppState>,
    principal: Principal,
    Query(params): Query<AuditCursorQuery>,
) -> Result<Json<ApiResponse<Vec<AuditEntry>>>, AppError> {
    principal.require_admin()?;
    let events = state
        .store
        .list_audit_events(params.into())
        .await
        .map_err(AppError::from)?;
    Ok(ApiResponse::page(events))
}
