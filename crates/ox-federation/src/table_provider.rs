//! DataFusion `TableProvider` implementation backed by a
//! `DataSourceAdapter`.
//!
//! A `SourceTableProvider` wraps a single (adapter, table) pair: its
//! schema is resolved once at construction via
//! `DataSourceAdapter::describe_table`, and `scan` delegates to the
//! adapter's `scan` primitive (Phase 2 addition on the trait).
//!
//! Predicate and projection pushdown are surfaced through the
//! `supports_filters_pushdown` hook: by default we report `Inexact`,
//! which lets DataFusion pass filters to the adapter while still
//! applying them in the engine as a safety net. Each adapter may
//! later override to `Exact` once it has verified round-trip
//! semantics against its source dialect.
//!
//! The provider is deliberately single-table. Multi-table workspaces
//! register multiple providers with the `FederationContext`; fanning
//! out to multiple tables inside one provider would blur the
//! responsibility between "adapter introspection" (which already
//! enumerates tables) and "planner registration".

use std::any::Any;
use std::sync::Arc;

use arrow_schema::SchemaRef;
use async_trait::async_trait;
use datafusion::catalog::{Session, TableProvider};
use datafusion::common::{DataFusionError, Result as DfResult};
use datafusion::datasource::{MemTable, TableType};
use datafusion::logical_expr::{Expr, TableProviderFilterPushDown};
use datafusion::physical_plan::ExecutionPlan;

use ox_source::DataSourceAdapter;

/// Wraps a `DataSourceAdapter` + one of its tables into a DataFusion
/// `TableProvider`.
///
/// Construct with [`SourceTableProvider::try_new`]. The constructor
/// fetches the Arrow schema eagerly so the plan-time schema probe
/// never hits the source after registration; execution-time scans
/// still hit the source for rows.
pub struct SourceTableProvider {
    adapter: Arc<dyn DataSourceAdapter>,
    table_name: String,
    schema: SchemaRef,
}

impl std::fmt::Debug for SourceTableProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The adapter trait object does not implement `Debug` itself —
        // printing only the identifying fields keeps this honest and
        // satisfies the `Debug` bound DataFusion requires on
        // `TableProvider`.
        f.debug_struct("SourceTableProvider")
            .field("source_type", &self.adapter.source_type())
            .field("table_name", &self.table_name)
            .finish_non_exhaustive()
    }
}

impl SourceTableProvider {
    /// Create a provider for `table_name` on `adapter`. The adapter's
    /// `describe_table` is called once and translated into an Arrow
    /// `Schema` via
    /// [`ox_source::normalize::describe_to_arrow_schema`].
    pub async fn try_new(
        adapter: Arc<dyn DataSourceAdapter>,
        table_name: impl Into<String>,
    ) -> crate::FederationResult<Self> {
        let table_name = table_name.into();
        let table_def = adapter.describe_table(&table_name).await?;
        let schema = ox_source::normalize::describe_to_arrow_schema(
            adapter.source_type(),
            &table_def,
        );
        Ok(Self {
            adapter,
            table_name,
            schema: Arc::new(schema),
        })
    }

    /// Table name as seen from the adapter side — used by the scan
    /// call-through and by `FederationContext` when registering.
    pub fn table_name(&self) -> &str {
        &self.table_name
    }

    /// Convenience accessor: Arrow schema pinned at construction.
    pub fn arrow_schema(&self) -> &SchemaRef {
        &self.schema
    }
}

#[async_trait]
impl TableProvider for SourceTableProvider {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn schema(&self) -> SchemaRef {
        Arc::clone(&self.schema)
    }

    fn table_type(&self) -> TableType {
        TableType::Base
    }

    /// Declare `Inexact` for every filter: DataFusion may hand us the
    /// filter, but keeps its own filter pass on top. Each adapter
    /// promotes to `Exact` per-predicate once it has verified that the
    /// source dialect round-trips the expression faithfully — that
    /// upgrade lands in Phase 6.
    fn supports_filters_pushdown(
        &self,
        filters: &[&Expr],
    ) -> DfResult<Vec<TableProviderFilterPushDown>> {
        Ok(vec![TableProviderFilterPushDown::Inexact; filters.len()])
    }

    async fn scan(
        &self,
        state: &dyn Session,
        projection: Option<&Vec<usize>>,
        filters: &[Expr],
        limit: Option<usize>,
    ) -> DfResult<Arc<dyn ExecutionPlan>> {
        // Buffer the adapter output into a single `RecordBatch` and
        // delegate the ExecutionPlan construction to `MemTable`.
        // Projection is applied in the engine so adapters don't have
        // to understand DataFusion's column-index semantics; the
        // streaming + adapter-side projection upgrade lands when the
        // cost model is in.
        //
        // Filters are not yet threaded into the adapter primitive; the
        // engine still applies them. See `supports_filters_pushdown`.
        let _ = filters;

        let batch = self
            .adapter
            .scan(&self.table_name, None, limit)
            .await
            .map_err(|e| DataFusionError::External(Box::new(e)))?;

        let mem = MemTable::try_new(Arc::clone(&self.schema), vec![vec![batch]])?;
        mem.scan(state, projection, filters, limit).await
    }
}
