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
use ox_ontology::source_analysis::{
    AnalysisPhase, AnalysisWarning, WarningClass, WarningLevel, WarningScope,
};
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

/// User-facing intent for an analysis run.
///
/// Wire shape (the `kind` tag matches the [`IntrospectionKernel`]
/// method that will service it):
/// - `{ "kind": "all" }` — every table the source advertises.
/// - `{ "kind": "subset", "tables": [...] }` — pick a subset, cache
///   bypassed in both directions.
/// - `{ "kind": "extend", "tables": [...] }` — grow an existing
///   analysis with the named tables; baseline is supplied by the
///   caller when invoking the kernel.
/// - `{ "kind": "reduce", "tables": [...] }` — drop the named
///   tables (and the FKs that reference them) from a baseline; the
///   kernel never calls the adapter for this variant, it operates
///   on the supplied baseline only.
///
/// One enum unifies the four intents so every consumer (admin
/// federation registry, project create / extend / reduce, future
/// schedulers) speaks the same language. The variant has no
/// default — every caller picks `All` deliberately or supplies a
/// `Subset` / `Extend` / `Reduce` list, so a missing field can
/// never collapse into a silent full-warehouse sweep.
///
/// ADR-0067 (rejected): an audit recommended a per-adapter contract
/// test asserting every adapter implements `AnalyzeSelection`'s
/// four variants identically. Architecture review showed the
/// contract is enforced by construction — `DataSourceAdapter`
/// exposes only atomic primitives (`list_tables`, `describe_table`,
/// `count_rows`, `sample_column`, `list_foreign_keys`); selection
/// semantics live entirely in [`IntrospectionKernel::analyze`],
/// which composes those primitives. An adapter has no surface
/// through which it could re-interpret `AnalyzeSelection`. Adding
/// the test would verify the kernel against itself, not the
/// adapter contract; the existing two-adapter unit suite in
/// `kernel.rs::tests` already covers the kernel's dispatch.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, utoipa::ToSchema,
)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AnalyzeSelection {
    /// Every table the source advertises.
    All,
    /// Only the named tables. No baseline involved — the result
    /// stands on its own.
    Subset {
        tables: BTreeSet<String>,
    },
    /// Grow an existing baseline analysis with the named tables.
    /// Tables already present in the baseline are dropped silently
    /// — extension is "add what's new", not "rescan".
    Extend {
        tables: BTreeSet<String>,
    },
    /// Drop the named tables from an existing baseline analysis.
    /// Symmetric to `Extend` — the operator already pulled the
    /// tables in but later realised they are infrastructure /
    /// audit / log relations that don't belong in the ontology.
    /// The kernel returns the baseline minus the named tables and
    /// every foreign key that referenced them. ADR-0026.
    Reduce {
        tables: BTreeSet<String>,
    },
}

impl AnalyzeSelection {
    /// Lower to the kernel-facing [`TableSelection`] — used by
    /// callers that route their own baseline merge (so `Extend` and
    /// `Subset` collapse to the same `TableSelection::Subset`).
    /// `Reduce` lowers to an empty `Subset` because the kernel
    /// path that handles it (`analyze_reduction`) does not call
    /// the adapter — it operates entirely on the supplied baseline.
    pub fn as_table_selection(&self) -> TableSelection {
        match self {
            Self::All => TableSelection::All,
            Self::Subset { tables } | Self::Extend { tables } => {
                TableSelection::Subset(tables.clone())
            }
            Self::Reduce { .. } => TableSelection::Subset(BTreeSet::new()),
        }
    }

    /// Reject empty `Subset` / `Extend` / `Reduce` lists at the
    /// request boundary. `All` is always valid; the named-list
    /// variants must carry at least one table to express a
    /// meaningful intent.
    pub fn validate(&self) -> OxResult<()> {
        match self {
            Self::All => Ok(()),
            Self::Subset { tables } if tables.is_empty() => Err(OxError::Validation {
                field: "selection.tables".to_string(),
                message: "subset selection requires at least one table name".to_string(),
            }),
            Self::Extend { tables } if tables.is_empty() => Err(OxError::Validation {
                field: "selection.tables".to_string(),
                message: "extend selection requires at least one table name".to_string(),
            }),
            Self::Reduce { tables } if tables.is_empty() => Err(OxError::Validation {
                field: "selection.tables".to_string(),
                message: "reduce selection requires at least one table name".to_string(),
            }),
            Self::Subset { .. } | Self::Extend { .. } | Self::Reduce { .. } => Ok(()),
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

    /// Wrap a primitive failure into an [`AnalysisWarning`] the kernel
    /// can attach to its analysis report. Backends inspect the raw
    /// error and may refine `class` (e.g., recognise a BigQuery
    /// partition-filter rejection and promote `TableSkipped` →
    /// `BigQueryPartitionFilterRequired`), translate the user-facing
    /// `summary` to Korean, and bind an actionable `hint`.
    ///
    /// The default impl is the safe fallback: it produces a generic
    /// summary derived from the raw error and stores the full text
    /// as `detail` for expand-on-demand display.
    fn classify_warning(
        &self,
        level: WarningLevel,
        phase: AnalysisPhase,
        class: WarningClass,
        scope: WarningScope,
        error: &OxError,
    ) -> AnalysisWarning {
        AnalysisWarning::new(level, phase, class, scope).with_detail(error.to_string())
    }

    /// Cheap capability probe — `true` when the adapter implements a
    /// real [`scan`](Self::scan) (data materialisation for the
    /// federation planner), `false` when it only supports
    /// introspection. Defaults to `false` so a new adapter that
    /// forgets to override `scan` is also explicit about not
    /// supporting federation queries.
    ///
    /// Mapping registration consults this so the failure surface is
    /// "this adapter cannot back a federated link" at admin time
    /// rather than "scan() returned UnsupportedOperation" deep in
    /// the planner at query time.
    fn supports_scan(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(items: &[&str]) -> BTreeSet<String> {
        items.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn analyze_selection_all_is_always_valid() {
        AnalyzeSelection::All.validate().unwrap();
    }

    #[test]
    fn analyze_selection_subset_with_tables_is_valid() {
        AnalyzeSelection::Subset {
            tables: names(&["users", "orders"]),
        }
        .validate()
        .unwrap();
    }

    #[test]
    fn analyze_selection_extend_with_tables_is_valid() {
        AnalyzeSelection::Extend {
            tables: names(&["payments"]),
        }
        .validate()
        .unwrap();
    }

    #[test]
    fn analyze_selection_reduce_with_tables_is_valid() {
        AnalyzeSelection::Reduce {
            tables: names(&["audit_log"]),
        }
        .validate()
        .unwrap();
    }

    #[test]
    fn analyze_selection_reduce_empty_is_rejected() {
        let err = AnalyzeSelection::Reduce {
            tables: BTreeSet::new(),
        }
        .validate()
        .unwrap_err();
        assert!(matches!(err, OxError::Validation { ref field, .. } if field == "selection.tables"));
    }

    #[test]
    fn analyze_selection_reduce_lowers_to_empty_subset() {
        // Reduce never calls the adapter, so it lowers to an empty
        // subset — the kernel routes it to `reduce_baseline`
        // before any introspection primitive runs.
        let reduce = AnalyzeSelection::Reduce {
            tables: names(&["audit_log"]),
        };
        match reduce.as_table_selection() {
            TableSelection::Subset(s) => assert!(s.is_empty()),
            TableSelection::All => panic!("reduce must not lower to All"),
        }
    }

    #[test]
    fn analyze_selection_subset_empty_is_rejected() {
        let err = AnalyzeSelection::Subset {
            tables: BTreeSet::new(),
        }
        .validate()
        .unwrap_err();
        assert!(matches!(err, OxError::Validation { ref field, .. } if field == "selection.tables"));
    }

    #[test]
    fn analyze_selection_extend_empty_is_rejected() {
        let err = AnalyzeSelection::Extend {
            tables: BTreeSet::new(),
        }
        .validate()
        .unwrap_err();
        assert!(matches!(err, OxError::Validation { ref field, .. } if field == "selection.tables"));
    }

    #[test]
    fn analyze_selection_lowers_to_table_selection() {
        assert!(matches!(
            AnalyzeSelection::All.as_table_selection(),
            TableSelection::All
        ));
        let subset = AnalyzeSelection::Subset {
            tables: names(&["a", "b"]),
        };
        match subset.as_table_selection() {
            TableSelection::Subset(s) => assert_eq!(s, names(&["a", "b"])),
            TableSelection::All => panic!("subset should not lower to All"),
        }
    }
}
