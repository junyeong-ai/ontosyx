//! Source-analysis scope primitives — table selection and
//! draft-lifecycle scope state.
//!
//! - [`TableSelection`] — internal allow-list the kernel filters
//!   tables against before invoking the adapter.
//! - [`AnalyzeSelection`] — user-facing intent for an analysis run
//!   (`All`, `Subset`, `Extend`, `Reduce`, `Staged`).
//! - [`AnalysisScope`] + [`DeferredTable`] — per-draft scope state
//!   that survives across analyse / extend / reanalyze passes.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use ox_core::error::{OxError, OxResult};
use ox_ontology::source_analysis::{
    AnalysisPhase, AnalysisWarning, WarningClass, WarningLevel, WarningScope,
};

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
/// Selection semantics live entirely in
/// [`IntrospectionKernel::analyze`]; `DataSourceAdapter` exposes
/// only atomic primitives (`list_tables`, `describe_table`,
/// `count_rows`, `sample_column`, `list_foreign_keys`) so adapters
/// cannot re-interpret `AnalyzeSelection`.
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
    /// every foreign key that referenced them.
    Reduce {
        tables: BTreeSet<String>,
    },
    /// Introspect only the listed tables, then mark every other
    /// table the source advertises as `deferred` on the
    /// [`AnalysisScope`]. Kernel cost equals `Subset`; the
    /// distinction is the implicit defer of the unpicked.
    Staged {
        tables: BTreeSet<String>,
    },
}

impl AnalyzeSelection {
    /// Tables this selection adds to the project's modeled set.
    /// Empty for `Reduce` (it removes from the baseline) and for
    /// `All` when no source-side table list is yet known to the
    /// caller — the caller routes `All` through
    /// [`AnalysisScope::record_selection`] with the resolved table
    /// list so it lands as `included` rather than as an opaque
    /// "everything" sentinel.
    pub fn additive_tables(&self) -> &BTreeSet<String> {
        static EMPTY: std::sync::OnceLock<BTreeSet<String>> = std::sync::OnceLock::new();
        match self {
            Self::Subset { tables } | Self::Extend { tables } | Self::Staged { tables } => tables,
            Self::All | Self::Reduce { .. } => EMPTY.get_or_init(BTreeSet::new),
        }
    }

    /// Tables this selection removes from the project's modeled
    /// set. Empty for everything except `Reduce`.
    pub fn removal_tables(&self) -> &BTreeSet<String> {
        static EMPTY: std::sync::OnceLock<BTreeSet<String>> = std::sync::OnceLock::new();
        match self {
            Self::Reduce { tables } => tables,
            Self::All | Self::Subset { .. } | Self::Extend { .. } | Self::Staged { .. } => {
                EMPTY.get_or_init(BTreeSet::new)
            }
        }
    }

    /// Lower to the kernel-facing [`TableSelection`] — used by
    /// callers that route their own baseline merge (so `Extend` and
    /// `Subset` collapse to the same `TableSelection::Subset`).
    /// `Reduce` lowers to an empty `Subset` because the kernel
    /// path that handles it (`analyze_reduction`) does not call
    /// the adapter — it operates entirely on the supplied baseline.
    pub fn as_table_selection(&self) -> TableSelection {
        match self {
            Self::All => TableSelection::All,
            Self::Subset { tables } | Self::Extend { tables } | Self::Staged { tables } => {
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
            Self::Staged { tables } if tables.is_empty() => Err(OxError::Validation {
                field: "selection.tables".to_string(),
                message: "staged selection requires at least one table name".to_string(),
            }),
            Self::Subset { .. }
            | Self::Extend { .. }
            | Self::Reduce { .. }
            | Self::Staged { .. } => Ok(()),
        }
    }
}

// ---------------------------------------------------------------------------
// AnalysisScope — project-lifecycle scope state
// ---------------------------------------------------------------------------

/// Draft-lifecycle scope: which tables this project has modeled,
/// which are deliberately deferred (with a reason and optional
/// revisit date), which are auto-excluded by policy, and the schema
/// fingerprint of the last introspection so drift detection can
/// compare against a fresh snapshot.
///
/// `included` is the union of every `AnalyzeSelection::All` /
/// `Subset` / `Extend` that has run against the project; the design
/// pipeline writes here whenever a table actually contributes a
/// NodeType / EdgeType to the ontology. `deferred` is the operator's
/// explicit "skip for now" — the table is acknowledged but not
/// modeled; the FE renders these as `n / N` progress fractions and
/// offers a one-click promotion to `included`. `excluded_by_policy`
/// captures auto-excluded relations the system never proposes
/// (system catalogues, audit tables, temp relations).
#[derive(
    Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize,
    schemars::JsonSchema, utoipa::ToSchema,
)]
pub struct AnalysisScope {
    /// Tables that have contributed to the ontology.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub included: BTreeSet<String>,
    /// Tables the operator has acknowledged but explicitly skipped.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deferred: Vec<DeferredTable>,
    /// Tables the system filters out before the operator sees them
    /// (system catalogues, audit tables, ephemeral relations). The
    /// list is project-local rather than global so a workspace can
    /// override the policy on a per-project basis.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub excluded_by_policy: BTreeSet<String>,
    /// Per-table schema fingerprint as observed at the last
    /// introspection. Drift detection compares fresh fingerprints
    /// against these to flag tables whose columns have changed since
    /// the last analysis.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub fingerprints: std::collections::BTreeMap<String, String>,
    /// Inclusive lower bound on freshness — the moment the most
    /// recent extend / reanalyze finished. `None` for projects that
    /// have never analyzed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_introspected_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// One entry in [`AnalysisScope::deferred`] — a table the operator
/// explicitly chose not to model yet.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize,
    schemars::JsonSchema, utoipa::ToSchema,
)]
pub struct DeferredTable {
    pub table: String,
    /// Why the operator skipped this table — surfaced in the FE
    /// "deferred" tab so a future reviewer understands the call.
    /// Free-form because the operator's reasoning is the value;
    /// pinning a closed enum here would push every nuance into a
    /// "Custom(String)" escape hatch that does the same job.
    pub reason: String,
    pub deferred_at: chrono::DateTime<chrono::Utc>,
    /// Optional reminder timestamp the FE uses to surface stale
    /// deferrals. `None` means "indefinite, the operator decides".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revisit_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl AnalysisScope {
    /// Apply an [`AnalyzeSelection`] to the scope.
    ///
    /// `all_tables_for_all_selection` is consulted only when the
    /// selection is `AnalyzeSelection::All` — the caller knows the
    /// resolved table list (from a fresh introspection or the prior
    /// project schema) and threads it in so the scope ingests every
    /// table by name rather than carrying an opaque "everything"
    /// flag downstream. Pass an empty set when the table list isn't
    /// yet known; `record_introspected_tables` then fills it once
    /// the introspection completes.
    ///
    /// `now` is the timestamp the caller wants stamped on
    /// `last_introspected_at` and on any `Reduce`-driven
    /// `DeferredTable`. Threading it in keeps the function pure and
    /// deterministic for tests.
    pub fn record_selection(
        &mut self,
        selection: &AnalyzeSelection,
        all_tables_for_all_selection: &BTreeSet<String>,
        now: chrono::DateTime<chrono::Utc>,
    ) {
        match selection {
            AnalyzeSelection::All => {
                for t in all_tables_for_all_selection {
                    self.include_one(t);
                }
            }
            AnalyzeSelection::Subset { tables } | AnalyzeSelection::Extend { tables } => {
                for t in tables {
                    self.include_one(t);
                }
            }
            AnalyzeSelection::Staged { tables } => {
                for t in tables {
                    self.include_one(t);
                }
                self.defer_remaining(
                    all_tables_for_all_selection,
                    "deferred at bootstrap",
                    now,
                );
            }
            AnalyzeSelection::Reduce { tables } => {
                for t in tables {
                    self.included.remove(t);
                    if !self.deferred.iter().any(|d| &d.table == t) {
                        self.deferred.push(DeferredTable {
                            table: t.clone(),
                            reason: "removed via reduce".into(),
                            deferred_at: now,
                            revisit_at: None,
                        });
                    }
                }
            }
        }
        self.last_introspected_at = Some(now);
    }

    /// Promote a table from `deferred` (or first-time-seen) into
    /// `included`. Idempotent — re-promoting a table that's already
    /// included is a no-op.
    fn include_one(&mut self, table: &str) {
        self.deferred.retain(|d| d.table != table);
        if !self.included.contains(table) {
            self.included.insert(table.to_string());
        }
    }

    /// Replace the per-table fingerprint snapshot in bulk. Existing
    /// entries are dropped — the caller hands in the post-
    /// introspection truth and the scope stays in sync.
    pub fn record_fingerprints(
        &mut self,
        fingerprints: impl IntoIterator<Item = (String, String)>,
    ) {
        self.fingerprints = fingerprints.into_iter().collect();
    }

    /// Add tables that aren't yet `included` or `deferred` to the
    /// `deferred` list with the supplied reason. Used by the
    /// "selective + acknowledge the rest" bootstrap flow so a
    /// curated subset implicitly defers everything the operator did
    /// not pick.
    pub fn defer_remaining(
        &mut self,
        all_tables: &BTreeSet<String>,
        reason: &str,
        now: chrono::DateTime<chrono::Utc>,
    ) {
        for t in all_tables {
            if self.included.contains(t) {
                continue;
            }
            if self.deferred.iter().any(|d| &d.table == t) {
                continue;
            }
            if self.excluded_by_policy.contains(t) {
                continue;
            }
            self.deferred.push(DeferredTable {
                table: t.clone(),
                reason: reason.to_string(),
                deferred_at: now,
                revisit_at: None,
            });
        }
    }

    /// Compare a fresh per-table fingerprint snapshot against the
    /// scope's stored baseline and emit one
    /// [`WarningClass::TableSchemaDrift`] warning per drift event.
    ///
    /// Two drift kinds surface (`params.kind`):
    /// - `"changed"` — table exists in both maps but the
    ///   fingerprints differ (column added / dropped / retyped,
    ///   nullability flipped, primary key shifted).
    /// - `"removed"` — table was in the prior baseline but is
    ///   missing from `fresh` (the table was dropped, renamed, or
    ///   moved out of the introspection's visible set).
    ///
    /// Tables present only in `fresh` are NEW observations rather
    /// than drift — the caller's `record_selection` flow ingests
    /// them through the normal include path. Pure function: same
    /// `(baseline, fresh)` always produces the same warnings, so
    /// re-runs over an unchanged source produce empty output.
    pub fn detect_drift(
        &self,
        fresh: &std::collections::BTreeMap<String, String>,
    ) -> Vec<AnalysisWarning> {
        let mut out = Vec::new();
        for (table, prior_fp) in &self.fingerprints {
            let kind = match fresh.get(table) {
                Some(fresh_fp) if fresh_fp == prior_fp => continue,
                Some(_) => "changed",
                None => "removed",
            };
            let scope = WarningScope::Table { name: table.clone() };
            out.push(
                AnalysisWarning::new(
                    WarningLevel::Warning,
                    AnalysisPhase::SchemaIntrospection,
                    WarningClass::TableSchemaDrift,
                    scope,
                )
                .with_param("kind", kind),
            );
        }
        out
    }
}
