use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use tracing::info;
use utoipa::ToSchema;
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

#[derive(Deserialize, ToSchema)]
pub struct ReviewRequest {
    pub approved: bool,
    /// Reviewer rationale recorded as the first comment on the
    /// approval thread when non-empty after trimming.
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct ReviewResponse {
    /// `"approved"` or `"rejected"`.
    pub status: String,
}

#[derive(Deserialize, ToSchema)]
pub struct CommentRequest {
    pub body: String,
}

// ---------------------------------------------------------------------------
// GET /api/approvals — list pending approvals for current workspace
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/approvals",
    tag = "Approvals",
    responses(
        (status = 200, description = "Pending approvals for the current workspace.", body = Vec<crate::openapi::ApprovalRequest>),
        (status = 401, description = "Unauthenticated.", body = crate::openapi::ErrorResponse),
    ),
    security(("api_key" = [])),
)]
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

#[utoipa::path(
    get,
    path = "/api/approvals/{id}",
    tag = "Approvals",
    params(("id" = Uuid, Path, description = "Approval request id")),
    responses(
        (status = 200, description = "The approval request.", body = crate::openapi::ApprovalRequest),
        (status = 404, description = "Not found.", body = crate::openapi::ErrorResponse),
    ),
    security(("api_key" = [])),
)]
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

#[utoipa::path(
    post,
    path = "/api/approvals/{id}/review",
    tag = "Approvals",
    params(("id" = Uuid, Path, description = "Approval request id")),
    request_body = ReviewRequest,
    responses(
        (status = 200, description = "Decision recorded.", body = ReviewResponse),
        (status = 403, description = "Workspace admin required.", body = crate::openapi::ErrorResponse),
        (status = 404, description = "No pending approval with this id.", body = crate::openapi::ErrorResponse),
    ),
    security(("api_key" = [])),
)]
pub(crate) async fn review_approval(
    State(state): State<AppState>,
    principal: Principal,
    ws: WorkspaceContext,
    Path(id): Path<Uuid>,
    Json(req): Json<ReviewRequest>,
) -> Result<(StatusCode, Json<ApiResponse<ReviewResponse>>), AppError> {
    ws.require_admin()?;
    let reviewer_id = principal.user_uuid()?;

    state
        .store
        .review_approval(id, reviewer_id, req.approved, req.note.as_deref())
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
        ApiResponse::of(ReviewResponse {
            status: status.to_string(),
        }),
    ))
}

// ---------------------------------------------------------------------------
// GET /api/approvals/:id/comments — list the comment thread
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/approvals/{id}/comments",
    tag = "Approvals",
    params(("id" = Uuid, Path, description = "Approval request id")),
    responses(
        (status = 200, description = "Thread of comments attached to this approval, oldest first.", body = Vec<crate::openapi::ApprovalComment>),
        (status = 404, description = "Not found.", body = crate::openapi::ErrorResponse),
    ),
    security(("api_key" = [])),
)]
pub(crate) async fn list_approval_comments(
    State(state): State<AppState>,
    _principal: Principal,
    _ws: WorkspaceContext,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiResponse<Vec<ApprovalComment>>>, AppError> {
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

#[utoipa::path(
    post,
    path = "/api/approvals/{id}/comments",
    tag = "Approvals",
    params(("id" = Uuid, Path, description = "Approval request id")),
    request_body = CommentRequest,
    responses(
        (status = 201, description = "Comment created and appended to the thread.", body = crate::openapi::ApprovalComment),
        (status = 400, description = "Empty body after trim.", body = crate::openapi::ErrorResponse),
        (status = 404, description = "Parent approval not found.", body = crate::openapi::ErrorResponse),
    ),
    security(("api_key" = [])),
)]
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
