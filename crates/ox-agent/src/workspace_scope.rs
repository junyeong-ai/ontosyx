//! `WorkspaceScope` — propagates workspace identity into every tool
//! dispatch so the store's RLS pool sees the right tenant.
//!
//! Implements [`entelix::tools::ToolDispatchScope`] (the canonical
//! entelix hook for tool-future wrapping) and re-applies the four
//! ontosyx task-locals on every tool dispatch:
//! `ox_store::WORKSPACE_ID`, `ox_store::SYSTEM_BYPASS`,
//! `ox_graph_runtime::GRAPH_WORKSPACE_ID`,
//! `ox_graph_runtime::GRAPH_SYSTEM_BYPASS`.
//!
//! Attach via [`entelix::tools::ScopedToolLayer::new`] on the tool
//! registry at agent build time:
//!
//! ```ignore
//! let registry = ToolRegistry::<()>::new()
//!     .layer(ScopedToolLayer::new(WorkspaceScope::new(mode)))
//!     .register(Arc::new(QueryGraphTool::new(...).into_adapter()))?;
//! ```
//!
//! ## Spawn boundaries
//!
//! The layer fires only on `ToolRegistry::dispatch` futures. Anything
//! a tool spawns via `tokio::spawn` (the embedding sink, the recovery
//! sink, fire-and-forget signal capture) detaches from the dispatch
//! future and **must** re-apply the scope manually via
//! [`ox_context::ContextScope::capture_current`] +
//! [`ox_context::ContextScope::run`]. Both `EmbeddingSink` and
//! `RecoveryDetectionSink` already do this.

use entelix::ExecutionContext;
use entelix::Result;
use entelix::tools::ToolDispatchScope;
use futures::future::BoxFuture;
use serde_json::Value;

use ox_context::{ContextScope, WorkspaceMode};

/// Tool-dispatch scope that re-applies one [`WorkspaceMode`] to every
/// dispatched tool future. Cheap to clone — the mode is `Copy`.
#[derive(Clone, Copy, Debug)]
pub struct WorkspaceScope {
    mode: WorkspaceMode,
}

impl WorkspaceScope {
    /// Build a scope bound to one mode. Construction is cheap; one
    /// instance per agent build is the typical wiring.
    #[must_use]
    pub const fn new(mode: WorkspaceMode) -> Self {
        Self { mode }
    }

    /// Bind the JWT-user path: every tool dispatch runs under
    /// `WORKSPACE_ID = id` + `GRAPH_WORKSPACE_ID = id`.
    #[must_use]
    pub const fn workspace(id: uuid::Uuid) -> Self {
        Self::new(WorkspaceMode::Workspace(id))
    }

    /// Bind the API-key / cron path: every tool dispatch runs under
    /// `SYSTEM_BYPASS` + `GRAPH_SYSTEM_BYPASS`.
    #[must_use]
    pub const fn system_bypass() -> Self {
        Self::new(WorkspaceMode::SystemBypass)
    }
}

impl ToolDispatchScope for WorkspaceScope {
    fn wrap(
        &self,
        _ctx: ExecutionContext,
        fut: BoxFuture<'static, Result<Value>>,
    ) -> BoxFuture<'static, Result<Value>> {
        let scope = ContextScope::new(self.mode);
        Box::pin(async move { scope.run(fut).await })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use entelix::tools::{ScopedToolLayer, Tool, ToolMetadata, ToolRegistry};
    use std::sync::Arc;

    use async_trait::async_trait;
    use entelix::AgentContext;
    use serde_json::json;
    use uuid::Uuid;

    /// Tool that captures whatever `WORKSPACE_ID` / `SYSTEM_BYPASS`
    /// it sees during dispatch — the `WorkspaceScope` test asserts
    /// the layer applies the scope before `Tool::execute` runs.
    struct ProbeTool {
        metadata: ToolMetadata,
    }

    impl ProbeTool {
        fn new() -> Self {
            Self {
                metadata: ToolMetadata::function(
                    "probe",
                    "Probe the active workspace task-local during dispatch.",
                    json!({ "type": "object" }),
                ),
            }
        }
    }

    #[async_trait]
    impl Tool for ProbeTool {
        fn metadata(&self) -> &ToolMetadata {
            &self.metadata
        }

        async fn execute(&self, _input: Value, _ctx: &AgentContext<()>) -> entelix::Result<Value> {
            let workspace = ox_store::WORKSPACE_ID.try_with(|id| *id).ok();
            let bypass = ox_store::SYSTEM_BYPASS.try_with(|v| *v).ok();
            let graph_workspace = ox_graph_runtime::GRAPH_WORKSPACE_ID.try_with(|id| *id).ok();
            let graph_bypass = ox_graph_runtime::GRAPH_SYSTEM_BYPASS.try_with(|v| *v).ok();
            Ok(json!({
                "workspace_id": workspace.map(|w| w.to_string()),
                "system_bypass": bypass,
                "graph_workspace_id": graph_workspace.map(|w| w.to_string()),
                "graph_system_bypass": graph_bypass,
            }))
        }
    }

    #[tokio::test]
    async fn workspace_mode_propagates_to_dispatch() {
        let id = Uuid::new_v4();
        let registry = ToolRegistry::new()
            .layer(ScopedToolLayer::new(WorkspaceScope::workspace(id)))
            .register(Arc::new(ProbeTool::new()))
            .unwrap();
        let ctx = ExecutionContext::default();
        let result = registry
            .dispatch("", "probe", json!({}), &ctx)
            .await
            .unwrap();
        assert_eq!(
            result["workspace_id"].as_str(),
            Some(id.to_string()).as_deref()
        );
        assert_eq!(
            result["graph_workspace_id"].as_str(),
            Some(id.to_string()).as_deref()
        );
        assert!(result["system_bypass"].is_null());
        assert!(result["graph_system_bypass"].is_null());
    }

    #[tokio::test]
    async fn system_bypass_mode_propagates_to_dispatch() {
        let registry = ToolRegistry::new()
            .layer(ScopedToolLayer::new(WorkspaceScope::system_bypass()))
            .register(Arc::new(ProbeTool::new()))
            .unwrap();
        let ctx = ExecutionContext::default();
        let result = registry
            .dispatch("", "probe", json!({}), &ctx)
            .await
            .unwrap();
        assert_eq!(result["system_bypass"], json!(true));
        assert_eq!(result["graph_system_bypass"], json!(true));
        assert!(result["workspace_id"].is_null());
        assert!(result["graph_workspace_id"].is_null());
    }
}
