//! Request-scope identity propagation across `tokio::spawn` and tower
//! middleware boundaries.
//!
//! Ontosyx scopes every database / graph call by four task-locals owned
//! by `ox-store` and `ox-graph-runtime`:
//!
//! - `ox_store::WORKSPACE_ID` — `Uuid` of the current workspace
//! - `ox_store::SYSTEM_BYPASS` — `bool` flag for system-level access
//! - `ox_graph_runtime::GRAPH_WORKSPACE_ID` — workspace for the graph layer
//! - `ox_graph_runtime::GRAPH_SYSTEM_BYPASS` — bypass for the graph layer
//!
//! The four task-locals come in two mutually-exclusive **modes**:
//! workspace-scoped (the JWT user path) sets both `WORKSPACE_ID`s;
//! system-bypass (the API-key / scheduled-task path) sets both
//! `SYSTEM_BYPASS` flags. Mixing the two — workspace + bypass on the same
//! future — is the worst-case audit footprint and is unrepresentable in
//! [`WorkspaceMode`] by construction.
//!
//! ## Why a separate crate
//!
//! Ax HTTP middleware sets the task-locals once at the request boundary.
//! Anything spawned afterwards (`tokio::spawn` inside a sink, a tool that
//! fans out work, the SSE keep-alive driver) runs in a fresh task without
//! those locals — calls into the store reach a connection without the
//! tenant-id binding and Postgres RLS denies every row. This crate's
//! [`ContextScope`] captures the four locals at the spawn site and
//! re-applies them inside the spawned future, restoring correctness
//! without leaking the task-local mechanism into every consumer.
//!
//! ## Usage
//!
//! Route boundary (explicit mode):
//!
//! ```ignore
//! let scope = ContextScope::new(WorkspaceMode::Workspace(workspace_id));
//! scope.run(handler_future).await
//! ```
//!
//! Sink / spawn boundary (capture from current task):
//!
//! ```ignore
//! let scope = ContextScope::capture_current();
//! tokio::spawn(scope.run(async move { /* sees the same task-locals */ }));
//! ```
//!
//! `ContextScope` is `Copy`, so passing it through a closure or an
//! `Arc<dyn _>` carries no allocation cost.

#![deny(missing_docs)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

mod progress;

pub use progress::{
    NoopProgressSink, ProgressContextExt, ProgressEvent, ProgressHandle, ProgressReporter,
    ProgressSink,
};

use std::future::Future;
use uuid::Uuid;

/// Mutually-exclusive request-scope identity.
///
/// `Workspace(id)` — JWT user path. RLS reads filter on the tenant;
/// writes stamp `workspace_id = id`.
///
/// `SystemBypass` — API-key / scheduled-task path. RLS-bypassing reads
/// (only authorised paths reach this) plus writes that intentionally
/// transcend a single workspace (tenant provisioning, cron sweeps).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkspaceMode {
    /// Scoped to a specific workspace.
    Workspace(Uuid),
    /// System-level access — bypasses workspace filters.
    SystemBypass,
}

/// Captured request-scope identity that can be re-applied to a future.
///
/// `mode == None` means neither task-local was set at capture time —
/// the contained future runs without re-scoping. This is the right
/// behaviour for unit tests and library code that may legitimately run
/// outside an HTTP request context; it is wrong for production paths,
/// where a missing scope indicates that middleware did not run.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ContextScope {
    mode: Option<WorkspaceMode>,
}

impl ContextScope {
    /// Build a scope with an explicit mode. Used at the request
    /// boundary where the JWT-vs-API-key decision happens.
    #[must_use]
    pub const fn new(mode: WorkspaceMode) -> Self {
        Self { mode: Some(mode) }
    }

    /// Capture the current task-locals. Returns an empty scope when
    /// neither `WORKSPACE_ID` nor `SYSTEM_BYPASS` is set on the calling
    /// task.
    ///
    /// `SYSTEM_BYPASS` takes precedence over `WORKSPACE_ID` — a path
    /// that has set both has explicitly opted out of workspace
    /// filtering.
    #[must_use]
    pub fn capture_current() -> Self {
        if let Ok(true) = ox_store::SYSTEM_BYPASS.try_with(|flag| *flag) {
            return Self {
                mode: Some(WorkspaceMode::SystemBypass),
            };
        }
        if let Ok(id) = ox_store::WORKSPACE_ID.try_with(|id| *id) {
            return Self {
                mode: Some(WorkspaceMode::Workspace(id)),
            };
        }
        Self { mode: None }
    }

    /// Borrow the captured mode. `None` means no task-local was active
    /// at capture time.
    #[must_use]
    pub const fn mode(&self) -> Option<WorkspaceMode> {
        self.mode
    }

    /// Run `fut` with the captured mode applied to all four task-locals.
    ///
    /// `Workspace(id)` sets `ox_store::WORKSPACE_ID` and
    /// `ox_graph_runtime::GRAPH_WORKSPACE_ID`. `SystemBypass` sets
    /// `ox_store::SYSTEM_BYPASS` and `ox_graph_runtime::GRAPH_SYSTEM_BYPASS`.
    /// An empty mode runs `fut` unmodified.
    pub async fn run<F>(self, fut: F) -> F::Output
    where
        F: Future + Send,
        F::Output: Send,
    {
        match self.mode {
            Some(WorkspaceMode::Workspace(id)) => {
                ox_store::WORKSPACE_ID
                    .scope(id, ox_graph_runtime::GRAPH_WORKSPACE_ID.scope(id, fut))
                    .await
            }
            Some(WorkspaceMode::SystemBypass) => {
                ox_store::SYSTEM_BYPASS
                    .scope(true, ox_graph_runtime::GRAPH_SYSTEM_BYPASS.scope(true, fut))
                    .await
            }
            None => fut.await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn capture_outside_any_scope_is_empty() {
        let scope = ContextScope::capture_current();
        assert_eq!(scope.mode(), None);
    }

    #[tokio::test]
    async fn capture_inside_workspace_scope_returns_workspace_mode() {
        let id = Uuid::new_v4();
        let captured = ox_store::WORKSPACE_ID
            .scope(id, async move { ContextScope::capture_current() })
            .await;
        assert_eq!(captured.mode(), Some(WorkspaceMode::Workspace(id)));
    }

    #[tokio::test]
    async fn capture_inside_system_bypass_scope_returns_bypass_mode() {
        let captured = ox_store::SYSTEM_BYPASS
            .scope(true, async move { ContextScope::capture_current() })
            .await;
        assert_eq!(captured.mode(), Some(WorkspaceMode::SystemBypass));
    }

    #[tokio::test]
    async fn system_bypass_wins_when_both_are_set() {
        let id = Uuid::new_v4();
        let captured = ox_store::SYSTEM_BYPASS
            .scope(true, async move {
                ox_store::WORKSPACE_ID
                    .scope(id, async move { ContextScope::capture_current() })
                    .await
            })
            .await;
        assert_eq!(captured.mode(), Some(WorkspaceMode::SystemBypass));
    }

    #[tokio::test]
    async fn workspace_run_applies_both_workspace_locals() {
        let id = Uuid::new_v4();
        let scope = ContextScope::new(WorkspaceMode::Workspace(id));
        scope
            .run(async move {
                let store = ox_store::WORKSPACE_ID.try_with(|v| *v).ok();
                let graph = ox_graph_runtime::GRAPH_WORKSPACE_ID.try_with(|v| *v).ok();
                assert_eq!(store, Some(id));
                assert_eq!(graph, Some(id));
            })
            .await;
    }

    #[tokio::test]
    async fn system_bypass_run_applies_both_bypass_locals() {
        let scope = ContextScope::new(WorkspaceMode::SystemBypass);
        scope
            .run(async move {
                let store = ox_store::SYSTEM_BYPASS.try_with(|v| *v).ok();
                let graph = ox_graph_runtime::GRAPH_SYSTEM_BYPASS.try_with(|v| *v).ok();
                assert_eq!(store, Some(true));
                assert_eq!(graph, Some(true));
            })
            .await;
    }

    #[tokio::test]
    async fn empty_scope_runs_future_unmodified() {
        let scope = ContextScope::default();
        let observed = scope
            .run(async { ox_store::WORKSPACE_ID.try_with(|v| *v).ok() })
            .await;
        assert_eq!(observed, None);
    }

    #[tokio::test]
    #[allow(
        clippy::disallowed_methods,
        reason = "test pins the spawn-propagation contract — `tokio::spawn` is the surface under test"
    )]
    async fn captured_scope_propagates_through_tokio_spawn() {
        let id = Uuid::new_v4();
        let observed = ox_store::WORKSPACE_ID
            .scope(id, async move {
                let scope = ContextScope::capture_current();
                let handle = tokio::spawn(
                    scope.run(async move { ox_store::WORKSPACE_ID.try_with(|v| *v).ok() }),
                );
                handle.await.ok().flatten()
            })
            .await;
        assert_eq!(observed, Some(id));
    }
}
