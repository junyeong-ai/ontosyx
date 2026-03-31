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
    tokio::spawn(async move {
        match scope {
            WsScope::System => {
                ox_store::SYSTEM_BYPASS
                    .scope(true, ox_runtime::GRAPH_SYSTEM_BYPASS.scope(true, fut))
                    .await;
            }
            WsScope::Workspace(id) => {
                ox_store::WORKSPACE_ID
                    .scope(id, ox_runtime::GRAPH_WORKSPACE_ID.scope(id, fut))
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
