//! Workspace-scoped federation context.
//!
//! A `FederationContext` is a thin wrapper around DataFusion's
//! `SessionContext` that carries the workspace identity and a set of
//! registered `SourceTableProvider`s. The API layer instantiates one
//! per request (cheap — DataFusion's `SessionContext::new` does no
//! I/O), registers the tables the query needs, then hands the
//! context to the planner.
//!
//! Phase 2 scope is deliberately small:
//! - `new(workspace_id)` → bare DataFusion session.
//! - `register_table(provider)` → adds the provider under its own
//!   table name.
//! - `sql(text)` → runs a raw SQL string for bring-up / integration
//!   tests. Phase 6 adds `plan(query_ir)`.

use std::sync::Arc;

use datafusion::prelude::SessionContext;

use crate::table_provider::SourceTableProvider;

/// The workspace identity carried on a federation request.
///
/// Wrapped in a newtype rather than a bare string so a future
/// migration to `Uuid` (or a task-local fetch from `ox-store`) doesn't
/// touch every call-site.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WorkspaceRef(String);

impl WorkspaceRef {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Per-request federation session. One context per request keeps
/// registered tables / UDFs / optimizer state isolated from sibling
/// requests; the DataFusion `SessionContext` is small enough to
/// recreate on every call.
pub struct FederationContext {
    workspace: WorkspaceRef,
    inner: SessionContext,
}

impl FederationContext {
    /// Build an empty federation context for `workspace`. DataFusion's
    /// default session config is used — Phase 6 may layer on
    /// concurrency / memory caps per workspace.
    pub fn new(workspace: WorkspaceRef) -> Self {
        Self {
            workspace,
            inner: SessionContext::new(),
        }
    }

    pub fn workspace(&self) -> &WorkspaceRef {
        &self.workspace
    }

    /// Register a table-provider under `provider.table_name()`. The
    /// provider's adapter is what runs at scan time; this call is
    /// schema-only.
    pub fn register_table(
        &self,
        provider: Arc<SourceTableProvider>,
    ) -> crate::FederationResult<()> {
        let name = provider.table_name().to_string();
        self.inner.register_table(&name, provider)?;
        Ok(())
    }

    /// Access the underlying `SessionContext` — test-only today; Phase
    /// 6 callers go through `plan(query_ir)` instead.
    #[doc(hidden)]
    pub fn session(&self) -> &SessionContext {
        &self.inner
    }

    /// Execute a raw SQL string against the registered tables, returning
    /// the materialised `RecordBatch`es. Present for bring-up and tests
    /// only — production callers go through a typed planner surface.
    pub async fn run_sql(
        &self,
        sql: &str,
    ) -> crate::FederationResult<Vec<datafusion::arrow::record_batch::RecordBatch>> {
        let df = self.inner.sql(sql).await?;
        let batches = df.collect().await?;
        Ok(batches)
    }

    /// Execute a DataFusion `LogicalPlan` directly. The primary
    /// production entry point — `run_sql` parses a string into a
    /// plan, but `execute_plan` takes the plan our own
    /// `LogicalPlanBuilder` has already produced, sparing the round
    /// trip through SQL text.
    ///
    /// Unlike `run_sql` this method does **not** require any
    /// `register_table` calls — the `LogicalPlan` produced by
    /// `build_match_plan` already embeds the concrete
    /// `TableProvider`s via `provider_as_source`. DataFusion walks
    /// the plan and reads rows directly from the providers.
    pub async fn execute_plan(
        &self,
        plan: datafusion::logical_expr::LogicalPlan,
    ) -> crate::FederationResult<Vec<datafusion::arrow::record_batch::RecordBatch>> {
        let df = self.inner.execute_logical_plan(plan).await?;
        let batches = df.collect().await?;
        Ok(batches)
    }
}
