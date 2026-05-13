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
//! Sink / spawn boundary (capture from current task, fire-and-forget):
//!
//! ```ignore
//! let scope = ContextScope::capture_current();
//! scope.spawn(async move { /* sees the same task-locals */ });
//! ```
//!
//! Common shortcuts (sugar):
//!
//! ```ignore
//! ox_context::spawn_scoped(async move { /* capture + spawn */ });
//! ox_context::spawn_system(async move { /* explicit SYSTEM_BYPASS spawn */ });
//! ```
//!
//! SSE / long-poll streams (re-enter on every `poll_next`):
//!
//! ```ignore
//! let scope = ContextScope::capture_current();
//! let stream = async_stream::stream! { /* store calls */ };
//! Sse::new(scope.scope_stream(stream))
//! ```
//!
//! `ContextScope` is `Copy`, so passing it through a closure or an
//! `Arc<dyn _>` carries no allocation cost. The `spawn` / `scope_stream`
//! methods are the single legitimate consumers of `tokio::spawn` and
//! the streaming scope re-entry — the workspace `clippy.toml` ban on
//! raw `tokio::spawn` keeps every other call site funnelled through
//! this crate.

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

    /// Spawn `fut` on a fire-and-forget task with the captured scope
    /// re-applied inside the spawned future. The single legitimate
    /// consumer of `tokio::spawn` for application code — the workspace
    /// `clippy.toml` ban makes any other caller fail the lint gate.
    ///
    /// Use this instead of `tokio::spawn(scope.run(fut))` so the
    /// safety invariant ("scope captured, scope re-applied") lives in
    /// one place and call sites stay free of lint suppression.
    pub fn spawn<F>(self, fut: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        // The whole purpose of this method is to be the central
        // adapter for the safe spawn pattern. The lint ban routes
        // every other caller through here; the allow is the one
        // legitimate exception.
        #[allow(
            clippy::disallowed_methods,
            reason = "ContextScope::spawn is the workspace's single approved adapter — \
                      scope.run(fut) re-applies WORKSPACE_ID / SYSTEM_BYPASS inside the \
                      spawned task so workspace-scoped store calls land under the right tenant"
        )]
        {
            drop(tokio::spawn(self.run(fut)));
        }
    }

    /// Wrap a [`Stream`](futures_core::Stream) so every `poll_next`
    /// re-enters the captured scope. SSE handlers are the canonical
    /// use-case — axum drives the stream *after* the request middleware
    /// scope has exited, so without this wrapper every store / runtime
    /// call inside the stream body sees no task-locals.
    ///
    /// ```ignore
    /// pub async fn handler() -> Sse<...> {
    ///     let scope = ContextScope::capture_current();
    ///     let inner = async_stream::stream! { /* store calls */ };
    ///     Sse::new(scope.scope_stream(inner)).keep_alive(...)
    /// }
    /// ```
    pub fn scope_stream<S>(self, inner: S) -> impl futures_core::Stream<Item = S::Item> + Send
    where
        S: futures_core::Stream + Send + 'static,
        S::Item: Send,
    {
        async_stream::stream! {
            let mut inner = Box::pin(inner);
            loop {
                // `ContextScope` is `Copy`, so re-using it across
                // every poll carries only the workspace UUID or
                // bypass flag — no allocation per item.
                let item: Option<S::Item> = self
                    .run(futures_core_next(&mut inner))
                    .await;
                match item {
                    Some(v) => yield v,
                    None => break,
                }
            }
        }
    }
}

/// Helper that adapts a pinned `Stream` to a `Future<Output = Option<Item>>`
/// for use inside [`ContextScope::scope_stream`]'s loop. Single
/// internal call-site, kept out of the public surface.
async fn futures_core_next<S>(stream: &mut std::pin::Pin<Box<S>>) -> Option<S::Item>
where
    S: futures_core::Stream + ?Sized,
{
    std::future::poll_fn(|cx| stream.as_mut().poll_next(cx)).await
}

/// Capture the current scope and spawn `fut` as a fire-and-forget
/// task with the captured task-locals re-applied. Equivalent to
/// `ContextScope::capture_current().spawn(fut)`; kept as a free
/// function because workspace-scoped spawn is the most common case
/// inside HTTP handlers and the brevity matters.
pub fn spawn_scoped<F>(fut: F)
where
    F: Future<Output = ()> + Send + 'static,
{
    ContextScope::capture_current().spawn(fut);
}

/// Spawn `fut` under explicit [`WorkspaceMode::SystemBypass`] —
/// cron sweeps, retention compaction, platform-wide maintenance.
/// Equivalent to `ContextScope::new(WorkspaceMode::SystemBypass).spawn(fut)`.
///
/// Never call this from an authenticated request path — use
/// [`spawn_scoped`] (workspace-preserving) instead.
pub fn spawn_system<F>(fut: F)
where
    F: Future<Output = ()> + Send + 'static,
{
    ContextScope::new(WorkspaceMode::SystemBypass).spawn(fut);
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

    use futures::StreamExt;

    #[tokio::test]
    async fn scope_stream_preserves_workspace_id_across_polls() {
        // SSE handler lifecycle: middleware sets the scope, the
        // handler captures it, the stream is driven AFTER the
        // middleware scope exits. Without `scope_stream`, every poll
        // sees a bare task-local and store calls inside the body land
        // unscoped.
        let ws_id = Uuid::new_v4();
        let (captured, raw_stream) = ox_store::WORKSPACE_ID
            .scope(ws_id, async {
                let captured = ContextScope::capture_current();
                let raw = async_stream::stream! {
                    for _ in 0..3 {
                        yield ox_store::WORKSPACE_ID.try_with(|id| *id).ok();
                    }
                };
                (captured, raw)
            })
            .await;
        assert!(
            ox_store::WORKSPACE_ID.try_with(|_| ()).is_err(),
            "post-scope baseline: task-local must be unset"
        );
        let wrapped = captured.scope_stream(raw_stream);
        let collected: Vec<Option<Uuid>> = Box::pin(wrapped).collect().await;
        assert_eq!(collected, vec![Some(ws_id), Some(ws_id), Some(ws_id)]);
    }

    #[tokio::test]
    async fn scope_stream_preserves_system_bypass_across_polls() {
        let (captured, raw_stream) = ox_store::SYSTEM_BYPASS
            .scope(true, async {
                let captured = ContextScope::capture_current();
                let raw = async_stream::stream! {
                    for _ in 0..2 {
                        yield ox_store::SYSTEM_BYPASS.try_with(|b| *b).ok();
                    }
                };
                (captured, raw)
            })
            .await;
        let wrapped = captured.scope_stream(raw_stream);
        let collected: Vec<Option<bool>> = Box::pin(wrapped).collect().await;
        assert_eq!(collected, vec![Some(true), Some(true)]);
    }

    #[tokio::test]
    async fn scope_stream_empty_mode_passes_through() {
        // Default scope (no captured mode) must be a no-op — useful
        // for test harnesses that build streams without ever setting
        // a workspace task-local.
        let scope = ContextScope::default();
        let raw = async_stream::stream! { yield 1; yield 2; };
        let collected: Vec<i32> = Box::pin(scope.scope_stream(raw)).collect().await;
        assert_eq!(collected, vec![1, 2]);
    }

    #[tokio::test]
    async fn spawn_scoped_function_inherits_workspace_id() {
        let ws_id = Uuid::new_v4();
        let observed = ox_store::WORKSPACE_ID
            .scope(ws_id, async move {
                let (tx, rx) = tokio::sync::oneshot::channel();
                super::spawn_scoped(async move {
                    let id = ox_store::WORKSPACE_ID.try_with(|v| *v).ok();
                    let _ = tx.send(id);
                });
                rx.await.unwrap_or(None)
            })
            .await;
        assert_eq!(observed, Some(ws_id));
    }

    #[tokio::test]
    async fn spawn_system_function_applies_bypass() {
        // Outside any scope, `spawn_system` still stamps the bypass
        // flag inside the spawned task.
        let (tx, rx) = tokio::sync::oneshot::channel();
        super::spawn_system(async move {
            let bypass = ox_store::SYSTEM_BYPASS.try_with(|b| *b).ok();
            let _ = tx.send(bypass);
        });
        assert_eq!(rx.await.ok().flatten(), Some(true));
    }
}
