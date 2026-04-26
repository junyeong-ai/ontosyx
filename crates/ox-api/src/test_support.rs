//! Wire-shape harness for axum handler tests.
//!
//! Drives a focused `axum::Router` end-to-end through `oneshot`:
//! the router carries only the route under test, the handler input
//! is a narrow `*State` (e.g. [`ApprovalsState`]) backed by a
//! per-trait stub, and the auth + workspace extensions the live
//! extractors read are injected directly via [`axum::Extension`]
//! layers. The test fires a `Request<Body>`, receives an HTTP
//! response, and parses the JSON body.
//!
//! ## Why a wire-shape harness
//!
//! Function-shape unit tests on `pub(crate) async fn list_approvals`
//! cover branching inside the handler but cannot catch the
//! regressions that ship most often: route-path typos, status-code
//! drift, response envelope shape. The harness here invokes the
//! router exactly as `axum::serve` does in production — extractors
//! run, body codecs run, response IntoResponse runs.
//!
//! ## Stub strategy
//!
//! The harness pulls a single sub-trait of [`ox_store::Store`]
//! (`ApprovalStore` for now) through a hand-rolled `StubApprovalStore`.
//! Methods the handler under test exercises are configurable; every
//! other method panics with a "not configured" message, which is
//! exactly how we catch a handler that started reaching into a
//! sub-trait beyond its declared dependency.
//!
//! ## Why hand-rolled instead of `mockall`
//!
//! `ApprovalStore::review_approval` carries a `note: Option<&str>`
//! argument. With `#[async_trait]`, mockall's `mock!` macro can't
//! reconcile the elided argument lifetimes against the async-trait
//! desugared `&self` lifetime — the generated impl complains about
//! "lifetimes in impl do not match this method in trait." A
//! hand-rolled trait stub fits the trait verbatim, so we accept the
//! one-time per-trait boilerplate (panic stubs for the methods the
//! handler under test doesn't exercise) in exchange for not
//! re-shaping the live trait signature. When a future trait without
//! borrowed arguments lands, `mockall` is the right tool — register
//! it as a workspace dev-dep at that point, not preemptively.

#![cfg(test)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::Extension;
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde::de::DeserializeOwned;
use tower::ServiceExt;
use uuid::Uuid;

use ox_core::error::OxResult;
use ox_store::{ApprovalComment, ApprovalRequest, ApprovalStore};

use crate::middleware::AuthClaims;
use crate::state::ApprovalsState;
use crate::workspace::{WorkspaceContext, WorkspaceRole};

// ---------------------------------------------------------------------------
// StubApprovalStore — focused stub for the ApprovalStore sub-trait
//
// `list_pending_approvals` is configurable per-test via
// `with_pending(...)`. Every other method panics on call so a
// handler that started reaching into a sub-trait method outside
// its declared dependency surfaces immediately as a test failure
// (instead of silently returning a default).
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct StubApprovalStore {
    pending: Mutex<Vec<ApprovalRequest>>,
    /// Captures the workspace_id passed to `list_pending_approvals` so
    /// tests can assert the handler routed RLS context correctly.
    last_listed_workspace: Mutex<Option<Uuid>>,
}

impl StubApprovalStore {
    pub fn with_pending(approvals: Vec<ApprovalRequest>) -> Arc<Self> {
        Arc::new(Self {
            pending: Mutex::new(approvals),
            last_listed_workspace: Mutex::new(None),
        })
    }

    pub fn last_listed_workspace(&self) -> Option<Uuid> {
        *self.last_listed_workspace.lock().unwrap()
    }
}

#[async_trait]
impl ApprovalStore for StubApprovalStore {
    async fn list_pending_approvals(
        &self,
        workspace_id: Uuid,
    ) -> OxResult<Vec<ApprovalRequest>> {
        *self.last_listed_workspace.lock().unwrap() = Some(workspace_id);
        Ok(self.pending.lock().unwrap().clone())
    }

    async fn create_approval_request(
        &self,
        _requester_id: Uuid,
        _action_type: &str,
        _resource_type: &str,
        _resource_id: &str,
        _payload: serde_json::Value,
    ) -> OxResult<ApprovalRequest> {
        panic!("StubApprovalStore::create_approval_request not configured for this test");
    }

    async fn get_approval_request(&self, _id: Uuid) -> OxResult<Option<ApprovalRequest>> {
        panic!("StubApprovalStore::get_approval_request not configured for this test");
    }

    async fn review_approval(
        &self,
        _id: Uuid,
        _reviewer_id: Uuid,
        _approved: bool,
        _note: Option<&str>,
    ) -> OxResult<Option<ApprovalComment>> {
        panic!("StubApprovalStore::review_approval not configured for this test");
    }

    async fn expire_old_approvals(&self) -> OxResult<Vec<(Uuid, u64)>> {
        panic!("StubApprovalStore::expire_old_approvals not configured for this test");
    }
}

// ---------------------------------------------------------------------------
// TestApp — the assembled harness
// ---------------------------------------------------------------------------

pub struct TestApp {
    router: Router,
}

impl TestApp {
    pub fn builder() -> TestAppBuilder {
        TestAppBuilder::default()
    }

    /// Drive the harness router with a single request and return
    /// the response status + parsed JSON body. Panics if the body
    /// is not well-formed JSON of `T` — handler tests want a tight
    /// signal here (any decoding failure is itself a regression).
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
// TestAppBuilder
// ---------------------------------------------------------------------------

pub struct TestAppBuilder {
    user_id: Option<Uuid>,
    workspace_id: Option<Uuid>,
    workspace_role: WorkspaceRole,
    approval_store: Option<Arc<dyn ApprovalStore>>,
}

impl Default for TestAppBuilder {
    fn default() -> Self {
        Self {
            user_id: None,
            workspace_id: None,
            workspace_role: WorkspaceRole::Admin,
            approval_store: None,
        }
    }
}

impl TestAppBuilder {
    /// Mark the caller as a workspace admin with the given user id.
    /// The harness still injects the lower-level `AuthClaims` so
    /// `Principal::from_request_parts` resolves the way it does
    /// against a live JWT.
    pub fn with_admin(mut self, user_id: Uuid) -> Self {
        self.user_id = Some(user_id);
        self.workspace_role = WorkspaceRole::Admin;
        self
    }

    pub fn with_workspace(mut self, workspace_id: Uuid) -> Self {
        self.workspace_id = Some(workspace_id);
        self
    }

    /// Install the approvals backend. Pass an `Arc<StubApprovalStore>`
    /// (or any other `ApprovalStore` impl).
    pub fn with_approval_store(mut self, store: Arc<dyn ApprovalStore>) -> Self {
        self.approval_store = Some(store);
        self
    }

    pub fn build(self) -> TestApp {
        let user_id = self.user_id.expect("TestApp::builder requires .with_admin(user_id)");
        let workspace_id = self
            .workspace_id
            .expect("TestApp::builder requires .with_workspace(ws_id)");
        let store = self
            .approval_store
            .expect("TestApp::builder requires .with_approval_store(...)");

        let auth_claims = AuthClaims {
            sub: user_id.to_string(),
            email: format!("{user_id}@test.local"),
            name: None,
            role: "admin".to_string(),
            iss: "ontosyx".to_string(),
            exp: 0,
            iat: 0,
        };
        let workspace_ctx = WorkspaceContext {
            workspace_id,
            workspace_role: self.workspace_role,
        };

        let router = Router::new()
            .route(
                "/api/approvals",
                axum::routing::get(crate::routes::approvals::list_approvals),
            )
            .layer(Extension(workspace_ctx))
            .layer(Extension(auth_claims))
            .with_state(ApprovalsState { store });

        TestApp { router }
    }
}
