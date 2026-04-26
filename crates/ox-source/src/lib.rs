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
pub mod config;
#[cfg(feature = "duckdb")]
pub mod duckdb_source;
pub mod fetcher;
pub mod kernel;
pub mod mongodb;
pub mod mysql;
pub mod normalize;
pub mod postgres;
pub mod postgres_fetcher;
pub mod json_scan;
pub mod registry;
pub mod repo;
pub mod sample;
pub mod snowflake;
pub mod text_scan;

pub use config::AdapterConfig;

use std::collections::BTreeSet;

use arrow_array::RecordBatch;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use ox_core::error::{OxError, OxResult};
use ox_ontology::source_analysis::AnalysisWarning;
use ox_core::source_schema::{
    ColumnStats, ForeignKeyDef, SchemaFingerprint, SourceColumnDef, SourceProfile, SourceSchema,
    SourceTableDef, TableSummary,
};

pub use kernel::{CacheTtl, IntrospectionKernel, RetryPolicy};

/// Which subset of an external source to introspect.
///
/// The selection lives at the call-site so the kernel can route a
/// single full sweep (`All`) and a user-driven partial sweep
/// (`Subset`) through the same primitive pipeline. Adapters never
/// see this enum directly — the kernel filters table names before
/// calling `describe_table` / `count_rows` / `sample_column`, so
/// adapters keep their atomic shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TableSelection {
    /// Every table the source advertises. The legacy "full scan"
    /// behaviour, made explicit so the call-site spells the intent.
    All,
    /// Only the named tables. Names not present in the source are
    /// dropped silently — selection is allow-list semantics, not
    /// validation.
    Subset(BTreeSet<String>),
}

impl TableSelection {
    /// Convenience: build a `Subset` from any iterator of stringy
    /// items. `Subset(BTreeSet::new())` yields a selection that
    /// matches no tables — a valid but rarely useful state.
    pub fn subset<I, S>(names: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self::Subset(names.into_iter().map(Into::into).collect())
    }

    /// Whether the selection includes a given table name.
    pub fn includes(&self, table: &str) -> bool {
        match self {
            Self::All => true,
            Self::Subset(set) => set.contains(table),
        }
    }
}

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
#[derive(Debug, Clone, Serialize, Deserialize)]
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

    /// Enumerate every table with cheap backend-native metadata
    /// (estimated row count, column count, last-modified) — meant for
    /// **selection UIs** where the user picks which subset of a source
    /// to introspect without paying the per-table profiling cost.
    ///
    /// Each adapter MUST implement this primitive against its
    /// backend's fast catalog path (`pg_stat_user_tables`,
    /// MySQL `information_schema.TABLES`, MongoDB
    /// `estimatedDocumentCount`, BigQuery `__TABLES__`, etc.). There
    /// is intentionally no default impl: silently falling back to
    /// `list_tables` + `describe_table` would defeat the purpose of
    /// the primitive (cheap previews on 1000-table sources).
    async fn list_tables_with_summary(&self) -> OxResult<Vec<TableSummary>>;

    /// Stable hash of a single table's column shape — used by the
    /// kernel to detect schema drift between two introspection runs
    /// without re-describing every table.
    ///
    /// The default impl derives a fingerprint from `describe_table`
    /// output via [`SchemaFingerprint::from_table`]. Adapters that
    /// can serve a backend-native fingerprint (a `SHOW TABLE STATUS`
    /// checksum, a Snowflake `INFORMATION_SCHEMA.TABLES.LAST_DDL`
    /// timestamp combined with a column hash) override this primitive
    /// to skip the full describe round-trip.
    async fn schema_fingerprint(&self, table: &str) -> OxResult<SchemaFingerprint> {
        let described = self.describe_table(table).await?;
        Ok(SchemaFingerprint::from_table(&described))
    }

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
    async fn sample_column(&self, table: &str, column: &SourceColumnDef) -> OxResult<ColumnStats>;

    /// Enumerate declared or inferred foreign-key relationships. Many
    /// backends don't declare FKs (CSV flat files, Mongo, most JSON)
    /// so the default returns an empty list; adapters that can
    /// discover FKs override this.
    async fn list_foreign_keys(&self) -> OxResult<Vec<ForeignKeyDef>> {
        Ok(Vec::new())
    }

    /// Materialise rows from `table` as an Arrow `RecordBatch`, so the
    /// federation layer (`ox-federation`) can plug this adapter into
    /// DataFusion's `TableProvider` surface.
    ///
    /// Contract:
    /// - `projection`, when `Some`, is a list of column indices into
    ///   the schema returned by `describe_table`. Adapters SHOULD push
    ///   the projection down to the source when the dialect allows it
    ///   and fall back to returning every column otherwise (DataFusion
    ///   re-projects on top).
    /// - `limit`, when `Some`, caps the number of rows the adapter
    ///   returns. It is advisory — returning fewer rows is always
    ///   correct; returning more means the federation engine has to
    ///   truncate.
    /// - Filters are not part of this primitive in Phase 2. DataFusion
    ///   still applies them after scan (we report
    ///   `TableProviderFilterPushDown::Inexact`). Phase 6 lifts filters
    ///   into this signature and lets adapters promote to `Exact`.
    ///
    /// The default implementation refuses — adapters without a scan
    /// path (e.g. a FK-only introspection stub) stay explicit about
    /// not supporting federation queries.
    async fn scan(
        &self,
        table: &str,
        _projection: Option<Vec<usize>>,
        _limit: Option<usize>,
    ) -> OxResult<RecordBatch> {
        Err(OxError::UnsupportedOperation {
            target: self.source_type().to_string(),
            operation: format!("scan(table={table})"),
        })
    }
}
