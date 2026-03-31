use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use tracing::info;
use uuid::Uuid;

use ox_store::ApprovalRequest;

use crate::error::AppError;
use crate::principal::Principal;
use crate::state::AppState;
use crate::workspace::WorkspaceContext;

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct ApprovalListResponse {
    pub approvals: Vec<ApprovalRequest>,
}

#[derive(Deserialize)]
pub struct ReviewRequest {
    pub approved: bool,
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Serialize)]
pub struct ReviewResponse {
    pub status: String,
}

// ---------------------------------------------------------------------------
// POST /api/approvals — list pending approvals for current workspace
// ---------------------------------------------------------------------------

pub(crate) async fn list_approvals(
    State(state): State<AppState>,
    _principal: Principal,
    ws: WorkspaceContext,
) -> Result<Json<ApprovalListResponse>, AppError> {
    let approvals = state
        .store
        .list_pending_approvals(ws.workspace_id)
        .await
        .map_err(AppError::from)?;

    Ok(Json(ApprovalListResponse { approvals }))
}

// ---------------------------------------------------------------------------
// GET /api/approvals/:id — get a single approval request
// ---------------------------------------------------------------------------

pub(crate) async fn get_approval(
    State(state): State<AppState>,
    _principal: Principal,
    Path(id): Path<Uuid>,
) -> Result<Json<ApprovalRequest>, AppError> {
    let approval = state
        .store
        .get_approval_request(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Approval request"))?;

    Ok(Json(approval))
}

// ---------------------------------------------------------------------------
// POST /api/approvals/:id/review — approve or reject
// ---------------------------------------------------------------------------

pub(crate) async fn review_approval(
    State(state): State<AppState>,
    principal: Principal,
    ws: WorkspaceContext,
    Path(id): Path<Uuid>,
    Json(req): Json<ReviewRequest>,
) -> Result<(StatusCode, Json<ReviewResponse>), AppError> {
    // Only workspace admins can review approvals
    ws.require_admin()?;

    let reviewer_id = principal.user_uuid()?;

    state
        .store
        .review_approval(id, reviewer_id, req.approved, req.notes.as_deref())
        .await
        .map_err(AppError::from)?;

    let status = if req.approved { "approved" } else { "rejected" };

    info!(
        approval_id = %id,
        reviewer_id = %reviewer_id,
        decision = status,
        "Approval request reviewed"
    );

    Ok((
        StatusCode::OK,
        Json(ReviewResponse {
            status: status.to_string(),
        }),
    ))
}
