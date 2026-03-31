use axum::Json;
use axum::extract::{Query, State};

use ox_store::store::{CursorPage, CursorParams};
use ox_store::AuditEntry;

use crate::error::AppError;
use crate::state::AppState;

// ---------------------------------------------------------------------------
// GET /api/audit — list audit events (cursor-paginated)
// ---------------------------------------------------------------------------

pub(crate) async fn list_audit_events(
    State(state): State<AppState>,
    Query(params): Query<CursorParams>,
) -> Result<Json<CursorPage<AuditEntry>>, AppError> {
    let events = state
        .store
        .list_audit_events(params)
        .await
        .map_err(AppError::from)?;
    Ok(Json(events))
}
