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
#[cfg(feature = "bigquery")]
pub mod bigquery;
pub mod config;
#[cfg(feature = "duckdb")]
pub mod duckdb_source;
pub mod fetcher;
#[cfg(feature = "gcp-auth")]
pub mod gcp_auth;
pub mod json_scan;
pub mod kernel;
#[cfg(feature = "mongodb")]
pub mod mongodb;
pub mod mysql;
pub mod normalize;
pub mod postgres;
pub mod postgres_fetcher;
pub mod registry;
pub mod repo;
pub mod sample;
pub mod scope;
#[cfg(feature = "snowflake")]
pub mod snowflake;
pub mod text_scan;

pub use config::AdapterConfig;
pub use scope::{
    AnalysisScope, AnalyzeSelection, DeferredTable, TableSchemaDrift, TableSchemaDriftKind,
    TableSelection, table_schema_drift_warnings,
};

use arrow_array::RecordBatch;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use ox_core::error::{OxError, OxResult};
use ox_core::source_schema::{
    ColumnStats, ForeignKeyDef, SchemaFingerprint, SourceColumnDef, SourceProfile, SourceSchema,
    SourceTableDef, TableSummary,
};
use ox_ontology::source_analysis::{
    AnalysisPhase, AnalysisWarning, WarningClass, WarningLevel, WarningScope,
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
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
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
    /// - Filters are not part of this primitive yet. DataFusion still
    ///   applies them after scan (we report
    ///   `TableProviderFilterPushDown::Inexact`). A future iteration
    ///   lifts filters into this signature and lets adapters promote
    ///   to `Exact`.
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

    /// Declarative capability snapshot consumed by the federation
    /// planner. Adapters override this to declare predicate-pushdown
    /// support, partition awareness, and source-dialect emission for
    /// `LinkMappingKind::Computed` so the planner can lift safe
    /// filters into the source instead of buffering everything in
    /// DataFusion. The default mirrors the legacy "introspection only"
    /// stance — `supports_scan = false`, no pushdown, no partition
    /// awareness — so a new adapter that forgets to override stays
    /// explicit about its narrow surface.
    fn capabilities(&self) -> AdapterCapabilities {
        AdapterCapabilities {
            supports_scan: self.supports_scan(),
            predicate_pushdown: PredicatePushdown::None,
            limit_pushdown: false,
            aggregate_pushdown: false,
            partition_aware: false,
            computed_link_dialect: None,
        }
    }
}

/// Capability snapshot for one [`DataSourceAdapter`]. The federation
/// planner reads this to decide which predicates to push, whether to
/// emit `LIMIT` past the scan boundary, and whether the source can
/// host a `LinkMappingKind::Computed` predicate as a server-side
/// view. Every flag is read-mostly — adapters return the same shape
/// for the lifetime of the connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdapterCapabilities {
    pub supports_scan: bool,
    pub predicate_pushdown: PredicatePushdown,
    pub limit_pushdown: bool,
    pub aggregate_pushdown: bool,
    /// Adapter prunes scans by partition column when the planner
    /// supplies a literal predicate on it. `true` for BigQuery /
    /// Snowflake / Hive; `false` for adapters that always read the
    /// full relation.
    pub partition_aware: bool,
    /// Source-side dialect the adapter accepts when the planner emits
    /// a server-side view for a `LinkMappingKind::Computed`
    /// predicate. `None` when the adapter cannot host computed
    /// links — those mappings must refuse rather than evaluate the
    /// predicate in DataFusion.
    pub computed_link_dialect: Option<SqlDialect>,
}

/// Predicate-pushdown depth declared by an adapter. The federation
/// planner uses this to choose between `TableProviderFilterPushDown::
/// Exact` (adapter promises faithful round-trip) and `Inexact` (the
/// engine re-applies the filter in DataFusion).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PredicatePushdown {
    /// No pushdown — every predicate stays in DataFusion. Safe
    /// default; correct for any adapter.
    None,
    /// Equality on a single column with a literal RHS — sufficient
    /// for partition-pruning warehouses that index on equality.
    EqualityOnly,
    /// Equality + range comparisons (`<`, `<=`, `>`, `>=`,
    /// `BETWEEN`). Fits relational engines (PostgreSQL, MySQL,
    /// BigQuery) that index ranges natively.
    EqualityAndRange,
    /// Full SQL `WHERE` round-trip including `AND` / `OR` /
    /// `IS NULL` / `IN`. The adapter accepts arbitrary ANSI SQL
    /// boolean expressions.
    Full,
}

/// Source SQL dialect for `LinkMappingKind::Computed` view emission.
/// The federation planner consults the adapter's
/// `computed_link_dialect` to decide which dialect to emit when it
/// has a `Computed` predicate to push server-side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SqlDialect {
    PostgreSql,
    MySql,
    BigQuery,
    Snowflake,
    DuckDb,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

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
        assert!(
            matches!(err, OxError::Validation { ref field, .. } if field == "selection.tables")
        );
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
        assert!(
            matches!(err, OxError::Validation { ref field, .. } if field == "selection.tables")
        );
    }

    #[test]
    fn analyze_selection_extend_empty_is_rejected() {
        let err = AnalyzeSelection::Extend {
            tables: BTreeSet::new(),
        }
        .validate()
        .unwrap_err();
        assert!(
            matches!(err, OxError::Validation { ref field, .. } if field == "selection.tables")
        );
    }

    #[test]
    fn analyze_selection_staged_with_tables_is_valid() {
        AnalyzeSelection::Staged {
            tables: names(&["customers"]),
        }
        .validate()
        .unwrap();
    }

    #[test]
    fn analyze_selection_staged_empty_is_rejected() {
        let err = AnalyzeSelection::Staged {
            tables: BTreeSet::new(),
        }
        .validate()
        .unwrap_err();
        assert!(
            matches!(err, OxError::Validation { ref field, .. } if field == "selection.tables")
        );
    }

    #[test]
    fn analyze_selection_staged_lowers_to_subset() {
        // Same kernel path as Subset — the staged distinction lives
        // in `record_selection`, not in introspection.
        let staged = AnalyzeSelection::Staged {
            tables: names(&["customers", "orders"]),
        };
        match staged.as_table_selection() {
            TableSelection::Subset(s) => assert_eq!(s, names(&["customers", "orders"])),
            TableSelection::All => panic!("staged must not lower to All"),
        }
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

    // ---------- AnalysisScope ----------

    fn now() -> chrono::DateTime<chrono::Utc> {
        chrono::Utc::now()
    }

    #[test]
    fn scope_subset_includes_tables_and_stamps_timestamp() {
        let mut scope = AnalysisScope::default();
        let sel = AnalyzeSelection::Subset {
            tables: names(&["customers", "orders"]),
        };
        scope.record_selection(&sel, &BTreeSet::new(), now());
        assert_eq!(scope.included, names(&["customers", "orders"]));
        assert!(scope.last_introspected_at.is_some());
        assert!(scope.deferred.is_empty());
    }

    #[test]
    fn scope_extend_accumulates_across_calls() {
        let mut scope = AnalysisScope::default();
        scope.record_selection(
            &AnalyzeSelection::Subset {
                tables: names(&["customers"]),
            },
            &BTreeSet::new(),
            now(),
        );
        scope.record_selection(
            &AnalyzeSelection::Extend {
                tables: names(&["orders", "payments"]),
            },
            &BTreeSet::new(),
            now(),
        );
        assert_eq!(scope.included, names(&["customers", "orders", "payments"]));
    }

    #[test]
    fn scope_all_ingests_resolved_table_list() {
        let mut scope = AnalysisScope::default();
        let all_tables = names(&["a", "b", "c"]);
        scope.record_selection(&AnalyzeSelection::All, &all_tables, now());
        assert_eq!(scope.included, all_tables);
    }

    #[test]
    fn scope_reduce_moves_table_to_deferred_with_audit_reason() {
        let mut scope = AnalysisScope::default();
        scope.record_selection(
            &AnalyzeSelection::Subset {
                tables: names(&["customers", "orders"]),
            },
            &BTreeSet::new(),
            now(),
        );
        scope.record_selection(
            &AnalyzeSelection::Reduce {
                tables: names(&["orders"]),
            },
            &BTreeSet::new(),
            now(),
        );
        assert_eq!(scope.included, names(&["customers"]));
        assert_eq!(scope.deferred.len(), 1);
        assert_eq!(scope.deferred[0].table, "orders");
        assert_eq!(scope.deferred[0].reason, "removed via reduce");
    }

    #[test]
    fn scope_staged_includes_picks_and_defers_the_rest() {
        let mut scope = AnalysisScope::default();
        let all = names(&["customers", "orders", "audit_log", "drafts", "payments"]);
        let sel = AnalyzeSelection::Staged {
            tables: names(&["customers", "orders"]),
        };
        scope.record_selection(&sel, &all, now());

        // Picks land in `included`.
        assert_eq!(scope.included, names(&["customers", "orders"]));
        // The remainder lands in `deferred` with the bootstrap reason.
        let deferred_tables: BTreeSet<String> =
            scope.deferred.iter().map(|d| d.table.clone()).collect();
        assert_eq!(deferred_tables, names(&["audit_log", "drafts", "payments"]));
        for d in &scope.deferred {
            assert_eq!(d.reason, "deferred at bootstrap");
        }
        assert!(scope.last_introspected_at.is_some());
    }

    #[test]
    fn scope_staged_skips_tables_already_excluded_by_policy() {
        // `defer_remaining` (which Staged folds into) honours the
        // workspace's auto-exclusion list, so a Staged sweep does
        // not write `system_*` / audit catalogues into the
        // user-visible deferred list.
        let mut scope = AnalysisScope::default();
        scope.excluded_by_policy.insert("audit_log".into());
        let all = names(&["customers", "orders", "audit_log", "drafts"]);
        let sel = AnalyzeSelection::Staged {
            tables: names(&["customers"]),
        };
        scope.record_selection(&sel, &all, now());

        let deferred_tables: BTreeSet<String> =
            scope.deferred.iter().map(|d| d.table.clone()).collect();
        assert_eq!(deferred_tables, names(&["drafts", "orders"]));
        assert!(scope.excluded_by_policy.contains("audit_log"));
    }

    #[test]
    fn scope_re_including_a_deferred_table_clears_the_deferral() {
        let mut scope = AnalysisScope::default();
        scope.record_selection(
            &AnalyzeSelection::Subset {
                tables: names(&["orders"]),
            },
            &BTreeSet::new(),
            now(),
        );
        scope.record_selection(
            &AnalyzeSelection::Reduce {
                tables: names(&["orders"]),
            },
            &BTreeSet::new(),
            now(),
        );
        assert_eq!(scope.deferred.len(), 1);
        scope.record_selection(
            &AnalyzeSelection::Extend {
                tables: names(&["orders"]),
            },
            &BTreeSet::new(),
            now(),
        );
        assert!(scope.included.contains("orders"));
        assert!(scope.deferred.is_empty(), "promotion clears the deferral");
    }

    #[test]
    fn scope_defer_remaining_skips_included_excluded_and_already_deferred() {
        let mut scope = AnalysisScope::default();
        scope.record_selection(
            &AnalyzeSelection::Subset {
                tables: names(&["customers"]),
            },
            &BTreeSet::new(),
            now(),
        );
        scope.excluded_by_policy.insert("audit_log".into());
        scope.deferred.push(DeferredTable {
            table: "drafts".into(),
            reason: "explicit".into(),
            deferred_at: now(),
            revisit_at: None,
        });

        scope.defer_remaining(
            &names(&["customers", "orders", "audit_log", "drafts", "payments"]),
            "not picked at bootstrap",
            now(),
        );

        // Only the genuinely-new tables become deferred.
        let deferred_tables: BTreeSet<String> =
            scope.deferred.iter().map(|d| d.table.clone()).collect();
        assert_eq!(deferred_tables, names(&["drafts", "orders", "payments"]));
        // Original deferral reason for `drafts` is preserved.
        let drafts = scope.deferred.iter().find(|d| d.table == "drafts").unwrap();
        assert_eq!(drafts.reason, "explicit");
    }

    #[test]
    fn scope_record_fingerprints_replaces_prior_snapshot() {
        let mut scope = AnalysisScope::default();
        scope.record_fingerprints([
            ("customers".into(), "v1".into()),
            ("orders".into(), "v1".into()),
        ]);
        scope.record_fingerprints([("customers".into(), "v2".into())]);
        // Replaces wholesale — the second call is the post-
        // introspection truth, not a delta.
        assert_eq!(scope.fingerprints.len(), 1);
        assert_eq!(scope.fingerprints["customers"], "v2");
    }

    // ---------- AnalysisScope::detect_table_schema_drift ----------

    fn fp_map<I: IntoIterator<Item = (&'static str, &'static str)>>(
        entries: I,
    ) -> std::collections::BTreeMap<String, String> {
        entries
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn detect_drift_returns_empty_when_fingerprints_match() {
        let mut scope = AnalysisScope::default();
        scope.record_fingerprints([("customers".into(), "v1".into())]);
        let fresh = fp_map([("customers", "v1")]);
        assert!(scope.detect_table_schema_drift(&fresh).is_empty());
    }

    #[test]
    fn detect_drift_flags_changed_fingerprint() {
        let mut scope = AnalysisScope::default();
        scope.record_fingerprints([("customers".into(), "v1".into())]);
        let fresh = fp_map([("customers", "v2")]);

        let drift = scope.detect_table_schema_drift(&fresh);
        assert_eq!(drift.len(), 1);
        assert_eq!(drift[0].table, "customers");
        assert_eq!(drift[0].kind, TableSchemaDriftKind::Changed);
    }

    #[test]
    fn detect_drift_flags_table_disappearing_from_fresh() {
        let mut scope = AnalysisScope::default();
        scope.record_fingerprints([
            ("customers".into(), "v1".into()),
            ("orders".into(), "v1".into()),
        ]);
        let fresh = fp_map([("customers", "v1")]);

        let drift = scope.detect_table_schema_drift(&fresh);
        assert_eq!(drift.len(), 1);
        assert_eq!(drift[0].table, "orders");
        assert_eq!(drift[0].kind, TableSchemaDriftKind::Removed);
    }

    #[test]
    fn detect_drift_ignores_tables_only_in_fresh() {
        // First-time observations are not drift — `record_selection`
        // ingests them through the normal include path.
        let scope = AnalysisScope::default();
        let fresh = fp_map([("customers", "v1")]);
        assert!(scope.detect_table_schema_drift(&fresh).is_empty());
    }

    #[test]
    fn detect_drift_emits_one_warning_per_drifted_table() {
        let mut scope = AnalysisScope::default();
        scope.record_fingerprints([
            ("customers".into(), "v1".into()),
            ("orders".into(), "v1".into()),
            ("payments".into(), "v1".into()),
        ]);
        let fresh = fp_map([("customers", "v2"), ("orders", "v1")]);

        let drift = scope.detect_table_schema_drift(&fresh);
        assert_eq!(drift.len(), 2);
        // Iteration order is BTreeMap-sorted: customers (changed),
        // payments (removed).
        assert_eq!(drift[0].kind, TableSchemaDriftKind::Changed);
        assert_eq!(drift[1].kind, TableSchemaDriftKind::Removed);
    }
}
