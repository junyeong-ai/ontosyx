use axum::Json;
use axum::extract::{Query, State};

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

pub(crate) async fn list_audit_events(
    State(state): State<AppState>,
    principal: Principal,
    Query(params): Query<CursorParams>,
) -> Result<Json<ApiResponse<Vec<AuditEntry>>>, AppError> {
    principal.require_admin()?;
    let events = state
        .store
        .list_audit_events(params)
        .await
        .map_err(AppError::from)?;
    Ok(ApiResponse::page(events))
}
