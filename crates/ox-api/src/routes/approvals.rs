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
use crate::state::{AppState, ApprovalsState};
use crate::workspace::WorkspaceContext;

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

#[derive(Deserialize, ToSchema)]
pub struct ReviewApprovalRequest {
    pub approved: bool,
    /// Reviewer rationale recorded as the first comment on the
    /// approval thread when non-empty after trimming.
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
#[cfg_attr(test, derive(Deserialize))]
pub struct ReviewApprovalResponse {
    /// `"approved"` or `"rejected"`.
    pub status: String,
}

#[derive(Deserialize, ToSchema)]
pub struct CreateApprovalCommentRequest {
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
    State(state): State<ApprovalsState>,
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
    request_body = ReviewApprovalRequest,
    responses(
        (status = 200, description = "Decision recorded.", body = ReviewApprovalResponse),
        (status = 403, description = "Workspace admin required.", body = crate::openapi::ErrorResponse),
        (status = 404, description = "No pending approval with this id.", body = crate::openapi::ErrorResponse),
    ),
    security(("api_key" = [])),
)]
pub(crate) async fn review_approval(
    State(state): State<ApprovalsState>,
    principal: Principal,
    ws: WorkspaceContext,
    Path(id): Path<Uuid>,
    Json(req): Json<ReviewApprovalRequest>,
) -> Result<(StatusCode, Json<ApiResponse<ReviewApprovalResponse>>), AppError> {
    ws.require_admin()?;
    let reviewer_id = principal.user_uuid()?;

    state
        .store
        .review_approval(id, reviewer_id, req.approved, req.note)
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
        ApiResponse::of(ReviewApprovalResponse {
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
    request_body = CreateApprovalCommentRequest,
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
    Json(req): Json<CreateApprovalCommentRequest>,
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

// ---------------------------------------------------------------------------
// Wire-shape tests — exercise routing + extractors + envelope shape
// through the focused harness in `crate::test_support`. Per-handler
// branching tests stay function-shape elsewhere.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::Router;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::routing::{get, post};
    use chrono::{TimeZone, Utc};
    use mockall::predicate::eq;
    use serde::Deserialize;
    use uuid::Uuid;

    use ox_store::ApprovalRequest;

    use crate::state::ApprovalsState;
    use crate::test_support::{
        MockApprovalStore, TestApp, admin_auth_layer, workspace_context_layer,
    };
    use crate::workspace::WorkspaceRole;

    #[derive(Deserialize)]
    struct ListEnvelope {
        data: Vec<ApprovalRequest>,
    }

    #[derive(Deserialize)]
    struct ReviewEnvelope {
        data: super::ReviewApprovalResponse,
    }

    fn fake_pending(workspace_id: Uuid, requester_id: Uuid) -> ApprovalRequest {
        let created = Utc.with_ymd_and_hms(2026, 4, 26, 12, 0, 0).unwrap();
        ApprovalRequest {
            id: Uuid::new_v4(),
            workspace_id,
            requester_id,
            requester_name: Some("Requester".to_string()),
            action_type: "schema_deploy".to_string(),
            resource_type: "ontology".to_string(),
            resource_id: Uuid::new_v4().to_string(),
            payload: serde_json::json!({}),
            status: "pending".to_string(),
            reviewer_id: None,
            reviewer_name: None,
            reviewed_at: None,
            expires_at: created + chrono::Duration::days(7),
            created_at: created,
        }
    }

    fn build_router(user_id: Uuid, workspace_id: Uuid, store: MockApprovalStore) -> Router {
        let state = ApprovalsState {
            store: Arc::new(store),
        };
        Router::new()
            .route("/api/approvals", get(super::list_approvals))
            .route("/api/approvals/{id}/review", post(super::review_approval))
            .layer(admin_auth_layer(user_id))
            .layer(workspace_context_layer(workspace_id, WorkspaceRole::Admin))
            .with_state(state)
    }

    #[tokio::test]
    async fn list_approvals_returns_pending_for_workspace() {
        let user_id = Uuid::new_v4();
        let workspace_id = Uuid::new_v4();
        let row = fake_pending(workspace_id, user_id);
        let row_clone = row.clone();

        let mut store = MockApprovalStore::new();
        store
            .expect_list_pending_approvals()
            .with(eq(workspace_id))
            .times(1)
            .returning(move |_| Ok(vec![row_clone.clone()]));

        let app = TestApp::new(build_router(user_id, workspace_id, store));
        let req = Request::builder()
            .method("GET")
            .uri("/api/approvals")
            .body(Body::empty())
            .unwrap();

        let (status, body): (StatusCode, ListEnvelope) = app.call_json(req).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body.data.len(), 1);
        assert_eq!(body.data[0].id, row.id);
        assert_eq!(body.data[0].workspace_id, workspace_id);
    }

    #[tokio::test]
    async fn list_approvals_returns_empty_envelope_when_no_pending() {
        let user_id = Uuid::new_v4();
        let workspace_id = Uuid::new_v4();

        let mut store = MockApprovalStore::new();
        store
            .expect_list_pending_approvals()
            .with(eq(workspace_id))
            .times(1)
            .returning(|_| Ok(Vec::new()));

        let app = TestApp::new(build_router(user_id, workspace_id, store));
        let req = Request::builder()
            .method("GET")
            .uri("/api/approvals")
            .body(Body::empty())
            .unwrap();

        let (status, body): (StatusCode, ListEnvelope) = app.call_json(req).await;
        assert_eq!(status, StatusCode::OK);
        assert!(body.data.is_empty());
    }

    #[tokio::test]
    async fn review_approval_records_decision_with_note() {
        let user_id = Uuid::new_v4();
        let workspace_id = Uuid::new_v4();
        let approval_id = Uuid::new_v4();

        let mut store = MockApprovalStore::new();
        store
            .expect_review_approval()
            .withf(move |id, _reviewer, approved, note| {
                *id == approval_id
                    && *approved
                    && note.as_deref() == Some("looks good")
            })
            .times(1)
            .returning(|_, _, _, _| Ok(None));

        let app = TestApp::new(build_router(user_id, workspace_id, store));
        let body = serde_json::json!({
            "approved": true,
            "note": "looks good",
        });
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/approvals/{approval_id}/review"))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();

        let (status, payload): (StatusCode, ReviewEnvelope) = app.call_json(req).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(payload.data.status, "approved");
    }
}
