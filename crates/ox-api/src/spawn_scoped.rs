// ---------------------------------------------------------------------------
// Workspace-aware spawn — fire-and-forget with explicit context propagation
// ---------------------------------------------------------------------------
// tokio::spawn creates a new task that does NOT inherit task-locals.
// SSE streams run after the middleware scope ends, so task-locals are gone.
//
// This module provides `spawn_with_ws` which takes an explicit WorkspaceScope
// and re-establishes task-locals inside the spawned task.
// ---------------------------------------------------------------------------

use std::future::Future;
use uuid::Uuid;

/// Workspace context captured at handler entry, before SSE streaming begins.
/// Passed to `spawn_with_ws` to propagate context into fire-and-forget tasks.
#[derive(Clone)]
pub enum WsScope {
    /// System-level access (API key users, scheduled tasks)
    System,
    /// Scoped to a specific workspace (normal JWT users)
    Workspace(Uuid),
    /// No context (migrations, startup)
    None,
}

impl WsScope {
    /// Capture the current workspace context from middleware task-locals.
    /// Call this in the handler BEFORE returning the SSE stream.
    pub fn capture() -> Self {
        if ox_store::SYSTEM_BYPASS.try_with(|b| *b).unwrap_or(false) {
            Self::System
        } else if let Ok(id) = ox_store::WORKSPACE_ID.try_with(|id| *id) {
            Self::Workspace(id)
        } else {
            Self::None
        }
    }
}

/// Spawn a fire-and-forget task with explicit workspace context.
///
/// Unlike `tokio::spawn`, this re-establishes SYSTEM_BYPASS/WORKSPACE_ID
/// task-locals inside the spawned task so DB operations succeed.
pub fn spawn_with_ws<F>(scope: WsScope, fut: F)
where
    F: Future<Output = ()> + Send + 'static,
{
    // The whole point of this module is to be the single legitimate caller
    // of `tokio::spawn`; the workspace-wide clippy gate forbids raw use
    // anywhere else.
    #[allow(clippy::disallowed_methods)]
    tokio::spawn(async move {
        match scope {
            WsScope::System => {
                ox_store::SYSTEM_BYPASS
                    .scope(true, ox_graph_runtime::GRAPH_SYSTEM_BYPASS.scope(true, fut))
                    .await;
            }
            WsScope::Workspace(id) => {
                ox_store::WORKSPACE_ID
                    .scope(id, ox_graph_runtime::GRAPH_WORKSPACE_ID.scope(id, fut))
                    .await;
            }
            WsScope::None => {
                fut.await;
            }
        }
    });
}

/// Convenience: capture + spawn in one call (works when called from middleware scope).
pub fn spawn_scoped<F>(fut: F)
where
    F: Future<Output = ()> + Send + 'static,
{
    spawn_with_ws(WsScope::capture(), fut);
}

/// Spawn a fire-and-forget **system** task — scheduled workers, retention
/// sweeps, rate-limiter cleanup, platform-wide maintenance loops.
///
/// Wraps the future in both `SYSTEM_BYPASS` task-locals so any store /
/// graph call inside the spawned task runs outside workspace isolation.
/// Equivalent to `spawn_with_ws(WsScope::System, fut)` but reads
/// clearly at the call-site as "this is intentional cross-tenant work".
///
/// Never call this from an authenticated request path — use
/// `spawn_scoped` (workspace-preserving) instead.
pub fn spawn_system<F>(fut: F)
where
    F: Future<Output = ()> + Send + 'static,
{
    spawn_with_ws(WsScope::System, fut);
}

/// Wrap a `Stream` so each `poll_next` re-enters the captured
/// workspace / bypass task-locals.
///
/// SSE handlers are the canonical use-case: middleware sets
/// `WORKSPACE_ID` / `GRAPH_WORKSPACE_ID` for the request's scope, the
/// handler returns an `Sse<Stream>`, and axum drives the Stream
/// *after* that scope has already exited. Without this wrapper,
/// every store / runtime call inside the stream body sees no
/// task-locals and either returns `MissingContext` (post-B6) or
/// silently fails RLS (pre-B6).
///
/// Use it like this:
///
/// ```ignore
/// pub async fn handler() -> Sse<...> {
///     let scope = WsScope::capture();
///     let inner = async_stream::stream! { /* state.store / brain calls */ };
///     Sse::new(scope_stream(scope, inner)).keep_alive(...)
/// }
/// ```
///
/// The capture happens *synchronously* before the handler returns,
/// so it sees the active scope. Each subsequent `next()` on the
/// returned stream re-enters that scope, making per-poll store
/// access work transparently.
pub fn scope_stream<S>(
    scope: WsScope,
    inner: S,
) -> impl futures_core::Stream<Item = S::Item> + Send
where
    S: futures_core::Stream + Send + 'static,
    S::Item: Send,
{
    async_stream::stream! {
        let mut inner = Box::pin(inner);
        loop {
            // `clone` is cheap (`Copy` for `Workspace`/`System`/`None`)
            // — clones of `WsScope` carry only the workspace UUID
            // or a discriminant byte.
            let item: Option<S::Item> = match scope.clone() {
                WsScope::Workspace(id) => {
                    ox_store::WORKSPACE_ID
                        .scope(
                            id,
                            ox_graph_runtime::GRAPH_WORKSPACE_ID
                                .scope(id, futures::StreamExt::next(&mut inner)),
                        )
                        .await
                }
                WsScope::System => {
                    ox_store::SYSTEM_BYPASS
                        .scope(
                            true,
                            ox_graph_runtime::GRAPH_SYSTEM_BYPASS
                                .scope(true, futures::StreamExt::next(&mut inner)),
                        )
                        .await
                }
                WsScope::None => futures::StreamExt::next(&mut inner).await,
            };
            match item {
                Some(v) => yield v,
                None => break,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    //! `scope_stream` regression test — proves the wrapper actually
    //! re-establishes `WORKSPACE_ID` on every poll. Without it, a
    //! Stream returned from an SSE handler runs after the request
    //! middleware's scope has already exited; every store call inside
    //! the stream body sees no task-locals and (post-B6) returns
    //! `MissingContext`.

    use super::*;
    use futures::StreamExt;
    use uuid::Uuid;

    #[tokio::test]
    async fn scope_stream_preserves_workspace_id_across_polls() {
        let ws_id = Uuid::new_v4();

        // Capture inside the scope, then build the stream OUTSIDE
        // the scope — mirrors the SSE-handler / axum lifecycle.
        let (captured, raw_stream) = ox_store::WORKSPACE_ID
            .scope(ws_id, async {
                let captured = WsScope::capture();
                let raw = async_stream::stream! {
                    for _ in 0..3 {
                        yield ox_store::WORKSPACE_ID.try_with(|id| *id).ok();
                    }
                };
                (captured, raw)
            })
            .await;

        // Sanity-check: outside the original scope, `WORKSPACE_ID`
        // is gone. The raw stream (without the wrapper) would yield
        // `None` here because each poll runs without the scope.
        assert!(
            ox_store::WORKSPACE_ID.try_with(|_| ()).is_err(),
            "post-scope baseline: task-local must be unset"
        );

        // The wrapped stream re-enters the scope on every poll.
        let wrapped = scope_stream(captured, raw_stream);
        let collected: Vec<Option<Uuid>> = Box::pin(wrapped).collect().await;
        assert_eq!(
            collected,
            vec![Some(ws_id), Some(ws_id), Some(ws_id)],
            "scope_stream must re-establish WORKSPACE_ID on every poll"
        );
    }

    #[tokio::test]
    async fn scope_stream_preserves_system_bypass_across_polls() {
        let (captured, raw_stream) = ox_store::SYSTEM_BYPASS
            .scope(true, async {
                let captured = WsScope::capture();
                let raw = async_stream::stream! {
                    for _ in 0..2 {
                        yield ox_store::SYSTEM_BYPASS.try_with(|b| *b).ok();
                    }
                };
                (captured, raw)
            })
            .await;

        let wrapped = scope_stream(captured, raw_stream);
        let collected: Vec<Option<bool>> = Box::pin(wrapped).collect().await;
        assert_eq!(collected, vec![Some(true), Some(true)]);
    }

    #[tokio::test]
    async fn scope_stream_with_none_passes_through() {
        // `WsScope::None` (no captured context) must still let the
        // stream finish — the wrapper is a no-op transformation, not
        // a hard failure.
        let raw = async_stream::stream! { yield 1; yield 2; };
        let wrapped = scope_stream(WsScope::None, raw);
        let collected: Vec<i32> = Box::pin(wrapped).collect().await;
        assert_eq!(collected, vec![1, 2]);
    }
}
