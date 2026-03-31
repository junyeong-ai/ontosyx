// ---------------------------------------------------------------------------
// WorkspaceContextScope — propagates workspace context into branchforge agent
// ---------------------------------------------------------------------------
// Implements branchforge::ContextScope to ensure task-locals (WORKSPACE_ID,
// SYSTEM_BYPASS, GRAPH_WORKSPACE_ID, GRAPH_SYSTEM_BYPASS) are available
// inside agent tool execution futures.
//
// Without this, parallel tool calls spawned by branchforge would lose the
// middleware-set task-locals, causing DB operations to fail with no workspace
// context (RLS deny-all).
// ---------------------------------------------------------------------------

use std::future::Future;
use std::pin::Pin;

use branchforge::{ContextScope, ToolResult};
use uuid::Uuid;

/// Propagates workspace isolation context into branchforge agent tool futures.
///
/// Two modes:
/// - **System bypass**: For API key users — sets SYSTEM_BYPASS + GRAPH_SYSTEM_BYPASS
/// - **Workspace scoped**: For JWT users — sets WORKSPACE_ID + GRAPH_WORKSPACE_ID
pub enum WorkspaceContextScope {
    /// System-level access (API key users, scheduled tasks)
    SystemBypass,
    /// Scoped to a specific workspace (normal JWT users)
    Workspace {
        workspace_id: Uuid,
    },
}

impl ContextScope for WorkspaceContextScope {
    fn wrap_tool_future<'a>(
        &'a self,
        fut: Pin<Box<dyn Future<Output = ToolResult> + Send + 'a>>,
    ) -> Pin<Box<dyn Future<Output = ToolResult> + Send + 'a>> {
        match self {
            WorkspaceContextScope::SystemBypass => {
                Box::pin(async move {
                    ox_store::SYSTEM_BYPASS
                        .scope(true, ox_runtime::GRAPH_SYSTEM_BYPASS.scope(true, fut))
                        .await
                })
            }
            WorkspaceContextScope::Workspace { workspace_id } => {
                let ws_id = *workspace_id;
                Box::pin(async move {
                    ox_store::WORKSPACE_ID
                        .scope(
                            ws_id,
                            ox_runtime::GRAPH_WORKSPACE_ID.scope(ws_id, fut),
                        )
                        .await
                })
            }
        }
    }
}
