//! Wire-shape harness for axum handler tests.
//!
//! [`TestApp`] wraps a fully-built `axum::Router` and drives it
//! through `Service::oneshot`, returning the response status and the
//! parsed JSON body. Callers assemble the router themselves — pick
//! the routes, attach the narrow per-handler state, and layer in the
//! authentication / workspace extensions via the helpers below.
//!
//! ## Composition recipe
//!
//! ```ignore
//! let mut store = MockApprovalStore::new();
//! store.expect_list_pending_approvals().returning(|_| Ok(vec![]));
//! let state = ApprovalsState { store: Arc::new(store) };
//! let router = Router::new()
//!     .route("/api/approvals", get(approvals::list_approvals))
//!     .layer(admin_auth_layer(user_id))
//!     .layer(workspace_context_layer(workspace_id, WorkspaceRole::Admin))
//!     .with_state(state);
//! let app = TestApp::new(router);
//! let (status, body) = app.call_json::<ListEnvelope>(req).await;
//! ```
//!
//! Per-trait stubs come from `mockall::mock!`. The
//! [`MockApprovalStore`] type below covers `ApprovalStore` for now;
//! sister mocks land in this module as new handlers grow tests.

#![cfg(test)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use async_trait::async_trait;
use axum::Extension;
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use mockall::mock;
use serde::de::DeserializeOwned;
use serde_json::Value;
use tower::ServiceExt;
use uuid::Uuid;

use ox_core::error::OxResult;
use ox_store::{ApprovalComment, ApprovalRequest, ApprovalStore};

use crate::middleware::AuthClaims;
use crate::workspace::{WorkspaceContext, WorkspaceRole};

mock! {
    pub ApprovalStore {}

    #[async_trait]
    impl ApprovalStore for ApprovalStore {
        async fn create_approval_request(
            &self,
            requester_id: Uuid,
            action_type: &str,
            resource_type: &str,
            resource_id: &str,
            payload: Value,
        ) -> OxResult<ApprovalRequest>;

        async fn get_approval_request(&self, id: Uuid) -> OxResult<Option<ApprovalRequest>>;

        async fn list_pending_approvals(
            &self,
            workspace_id: Uuid,
        ) -> OxResult<Vec<ApprovalRequest>>;

        async fn review_approval(
            &self,
            id: Uuid,
            reviewer_id: Uuid,
            approved: bool,
            note: Option<String>,
        ) -> OxResult<Option<ApprovalComment>>;

        async fn review_approvals(
            &self,
            ids: &[Uuid],
            reviewer_id: Uuid,
            approved: bool,
            note: Option<String>,
        ) -> OxResult<u64>;

        async fn expire_old_approvals(&self) -> OxResult<Vec<(Uuid, u64)>>;
    }
}

// ---------------------------------------------------------------------------
// TestApp — generic Router driver
// ---------------------------------------------------------------------------

pub struct TestApp {
    router: Router,
}

impl TestApp {
    pub fn new(router: Router) -> Self {
        Self { router }
    }

    pub async fn call_json<T: DeserializeOwned>(
        &self,
        req: Request<Body>,
    ) -> (StatusCode, T) {
        let resp = self.router.clone().oneshot(req).await.unwrap();
        let status = resp.status();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let parsed: T = serde_json::from_slice(&bytes).unwrap_or_else(|e| {
            panic!(
                "failed to parse JSON response body: {e}; raw body = {:?}",
                String::from_utf8_lossy(&bytes)
            )
        });
        (status, parsed)
    }
}

// ---------------------------------------------------------------------------
// Layer helpers — pre-built `Extension` layers the live extractors
// (`Principal`, `WorkspaceContext`) read from request extensions.
// ---------------------------------------------------------------------------

pub fn admin_auth_layer(user_id: Uuid) -> Extension<AuthClaims> {
    Extension(AuthClaims {
        sub: user_id.to_string(),
        email: format!("{user_id}@test.local"),
        name: None,
        role: "admin".to_string(),
        iss: "ontosyx".to_string(),
        exp: 0,
        iat: 0,
        jti: Uuid::new_v4(),
        tv: 0,
    })
}

pub fn workspace_context_layer(
    workspace_id: Uuid,
    role: WorkspaceRole,
) -> Extension<WorkspaceContext> {
    Extension(WorkspaceContext {
        workspace_id,
        workspace_role: role,
    })
}

