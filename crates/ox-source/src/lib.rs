#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unreachable
    )
)]

pub mod analyzer;
pub mod bigquery;
#[cfg(feature = "duckdb")]
pub mod duckdb_source;
pub mod fetcher;
pub mod kernel;
pub mod mongodb;
pub mod mysql;
pub mod postgres;
pub mod postgres_fetcher;
pub mod registry;
pub mod repo;
pub mod sample;
pub mod snowflake;

use async_trait::async_trait;
use ox_core::error::OxResult;
use ox_core::source_analysis::AnalysisWarning;
use ox_core::source_schema::{
    ColumnStats, ForeignKeyDef, SourceColumnDef, SourceProfile, SourceSchema, SourceTableDef,
};

pub use kernel::{CacheTtl, IntrospectionKernel, RetryPolicy};

/// Default concurrency limit for table introspection orchestration.
/// The [`IntrospectionKernel`] uses this when a caller doesn't override.
pub const DEFAULT_INTROSPECTION_CONCURRENCY: usize = 8;

/// Result of a full source analysis: schema, profile, and any warnings
/// encountered during introspection or profiling.
///
/// Produced by [`IntrospectionKernel::analyze`]. Adapters themselves never
/// emit this directly — they expose atomic primitives and the kernel
/// orchestrates them into a full analysis, attaching warnings as
/// individual primitive calls succeed or fail.
#[derive(Debug, Clone)]
pub struct AnalysisResult {
    pub schema: SourceSchema,
    pub profile: SourceProfile,
    pub warnings: Vec<AnalysisWarning>,
}

/// Introspect an external data source through a set of **atomic, read-only
/// primitives**. The [`IntrospectionKernel`] composes these primitives
/// into the higher-level flow (schema discovery → profiling → analysis)
/// while owning retry, concurrency, caching, and warning aggregation.
///
/// Every adapter implements exactly the same five methods (+ a default FK
/// primitive). Cross-cutting behaviour lives in one place; per-backend
/// code never re-implements it.
///
/// Primitives are expected to be idempotent and safe to retry — callers
/// get retry semantics from the kernel's `RetryPolicy`, and a mid-
/// introspection connection hiccup shouldn't leave server-side state
/// behind.
#[async_trait]
pub trait DataSourceAdapter: Send + Sync {
    /// Source type identifier (e.g., "postgresql", "mysql"). Cheap,
    /// synchronous accessor — no I/O.
    fn source_type(&self) -> &str;

    /// Enumerate every table (or collection) visible to this adapter.
    /// The returned names are fed to [`describe_table`] / [`count_rows`]
    /// / [`sample_column`] without further translation.
    async fn list_tables(&self) -> OxResult<Vec<String>>;

    /// Describe a single table: column metadata plus primary-key columns.
    /// Foreign keys are source-global and surface through
    /// [`list_foreign_keys`] instead — that split lets the kernel
    /// orchestrate them concurrently.
    async fn describe_table(&self, table: &str) -> OxResult<SourceTableDef>;

    /// Approximate row count for a table. Adapters should prefer a
    /// fast-path (e.g. PostgreSQL `pg_stat_user_tables`, MySQL InnoDB
    /// stats, Mongo `estimatedDocumentCount`) and only fall back to an
    /// exact count when statistics aren't available.
    async fn count_rows(&self, table: &str) -> OxResult<u64>;
    /// Profile a single column: null count, distinct count, sample
    /// values, min/max. Adapters fold these into the most efficient
    /// form their backend offers — a single aggregation query in SQL
    /// engines, in-memory aggregation for sampled documents, etc.
    async fn sample_column(
        &self,
        table: &str,
        column: &SourceColumnDef,
    ) -> OxResult<ColumnStats>;

    /// Enumerate declared or inferred foreign-key relationships. Many
    /// backends don't declare FKs (CSV flat files, Mongo, most JSON)
    /// so the default returns an empty list; adapters that can
    /// discover FKs override this.
    async fn list_foreign_keys(&self) -> OxResult<Vec<ForeignKeyDef>> {
        Ok(Vec::new())
    }
}
