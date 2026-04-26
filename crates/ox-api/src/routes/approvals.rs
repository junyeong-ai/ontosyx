use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use tracing::info;
use uuid::Uuid;

use ox_store::{ApprovalComment, ApprovalRequest};

use crate::error::AppError;
use crate::principal::Principal;
use crate::response::ApiResponse;
use crate::state::AppState;
use crate::workspace::WorkspaceContext;

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

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

#[derive(Deserialize)]
pub struct CommentRequest {
    pub body: String,
}

// ---------------------------------------------------------------------------
// POST /api/approvals — list pending approvals for current workspace
// ---------------------------------------------------------------------------

pub(crate) async fn list_approvals(
    State(state): State<AppState>,
    _principal: Principal,
    ws: WorkspaceContext,
) -> Result<Json<ApiResponse<Vec<ApprovalRequest>>>, AppError> {
    let approvals = state
        .store
        .list_pending_approvals(ws.workspace_id)
        .await
        .map_err(AppError::from)?;

    Ok(ApiResponse::of(approvals))
}

// ---------------------------------------------------------------------------
// GET /api/approvals/:id — get a single approval request
// ---------------------------------------------------------------------------

pub(crate) async fn get_approval(
    State(state): State<AppState>,
    _principal: Principal,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<ApprovalRequest>>, AppError> {
    let approval = state
        .store
        .get_approval_request(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Approval request"))?;

    Ok(ApiResponse::of(approval))
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
) -> Result<(StatusCode, Json<ApiResponse<ReviewResponse>>), AppError> {
    // Only workspace admins can review approvals
    ws.require_admin()?;

    let reviewer_id = principal.user_uuid()?;

    // Trim once and re-use for both the legacy review_notes column
    // and the thread comment so the two surfaces never disagree.
    let trimmed_note = req.notes.as_deref().map(str::trim).filter(|s| !s.is_empty());

    state
        .store
        .review_approval(id, reviewer_id, req.approved, trimmed_note)
        .await
        .map_err(AppError::from)?;

    // Persist the rationale on the thread too so post-decision
    // viewers see the decision-time note alongside any pre-decision
    // discussion. Failure to insert the comment must not fail the
    // review itself — the decision already landed.
    if let Some(body) = trimmed_note
        && let Err(err) = state.store.create_approval_comment(id, reviewer_id, body).await
    {
        tracing::warn!(
            approval_id = %id,
            reviewer_id = %reviewer_id,
            error = %err,
            "Failed to mirror review note to comment thread",
        );
    }

    let status = if req.approved { "approved" } else { "rejected" };

    info!(
        approval_id = %id,
        reviewer_id = %reviewer_id,
        decision = status,
        "Approval request reviewed"
    );

    Ok((
        StatusCode::OK,
        ApiResponse::of(ReviewResponse {
            status: status.to_string(),
        }),
    ))
}

// ---------------------------------------------------------------------------
// GET /api/approvals/:id/comments — list the comment thread
// ---------------------------------------------------------------------------

pub(crate) async fn list_approval_comments(
    State(state): State<AppState>,
    _principal: Principal,
    _ws: WorkspaceContext,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<Vec<ApprovalComment>>>, AppError> {
    // RLS scopes the rows to the caller's workspace; the parent
    // approval lookup serves as a 404-vs-403 distinguisher so the
    // caller gets a clear "not found" rather than an empty list when
    // the id is wrong.
    state
        .store
        .get_approval_request(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Approval request"))?;

    let comments = state
        .store
        .list_approval_comments(id)
        .await
        .map_err(AppError::from)?;

    Ok(ApiResponse::of(comments))
}

// ---------------------------------------------------------------------------
// POST /api/approvals/:id/comments — append a comment to the thread
// ---------------------------------------------------------------------------

pub(crate) async fn create_approval_comment(
    State(state): State<AppState>,
    principal: Principal,
    _ws: WorkspaceContext,
    Path(id): Path<Uuid>,
    Json(req): Json<CommentRequest>,
) -> Result<(StatusCode, Json<ApiResponse<ApprovalComment>>), AppError> {
    let body = req.body.trim();
    if body.is_empty() {
        return Err(AppError::bad_request("Comment body must not be empty"));
    }

    // Confirm the parent approval exists in the caller's workspace
    // before we insert. Without this, a typo'd id surfaces as a
    // generic FK-violation instead of a clean 404.
    state
        .store
        .get_approval_request(id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("Approval request"))?;

    let author_id = principal.user_uuid()?;
    let comment = state
        .store
        .create_approval_comment(id, author_id, body)
        .await
        .map_err(AppError::from)?;

    Ok((StatusCode::CREATED, ApiResponse::of(comment)))
}
