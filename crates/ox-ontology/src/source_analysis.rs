use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Adaptive thresholds — single source of truth for all large-schema policies
// ---------------------------------------------------------------------------

/// Table count threshold for analysis report warnings and LLM input compression.
pub const LARGE_SCHEMA_WARNING_THRESHOLD: usize = 50;

/// Maximum cardinality for a column to be treated as categorical/enum.
/// Columns at or below this threshold get ALL distinct values collected during
/// profiling, and ALL values preserved in LLM input (not truncated to 5 samples).
pub const ENUM_CARDINALITY_THRESHOLD: u64 = 100;

/// Table count threshold requiring explicit acknowledgement before design.
/// Also used for PostgreSQL introspection operational warnings.
pub const LARGE_SCHEMA_GATE_THRESHOLD: usize = 100;

/// Node count threshold for activating adaptive graph profile reduction.
pub const LARGE_ONTOLOGY_THRESHOLD: usize = 100;

// ---------------------------------------------------------------------------
// SourceAnalysisReport — result of programmatic pre-design analysis
// ---------------------------------------------------------------------------

/// Full analysis report produced by analyzing schema + profile before ontology design.
/// Contains findings ordered by actionability: implied FKs, PII, ambiguous columns, etc.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceAnalysisReport {
    /// Summary statistics about the schema
    pub schema_stats: SchemaStats,
    /// Potential foreign key relationships not declared in the schema
    pub implied_relationships: Vec<ImpliedRelationship>,
    /// Columns the classifier suggests carry PII. Suggestions are
    /// advisory; the operator confirms each by submitting a
    /// matching [`crate::pii::PiiAnnotation`] in [`DesignOptions`].
    pub pii_suggestions: Vec<crate::pii::PiiSuggestion>,
    /// Columns whose values are ambiguous and need user clarification.
    /// Each entry is a persistent [`crate::ambiguity::AmbiguityContext`]
    /// — the resolver path picks one of these up and attaches a
    /// resolution when the admin or agent decides the meaning.
    pub ambiguous_columns: Vec<crate::ambiguity::AmbiguityContext>,
    /// Tables suggested for exclusion from the ontology
    pub table_exclusion_suggestions: Vec<TableExclusionSuggestion>,
    /// Present when the schema is unusually large
    pub large_schema_warning: Option<LargeSchemaWarning>,
    /// Repo-sourced suggestions for ambiguous columns (user must explicitly accept)
    pub repo_suggestions: Vec<RepoColumnSuggestion>,
    /// Summary of repo analysis results (present only when repo was analyzed)
    pub repo_summary: Option<RepoAnalysisSummary>,
    /// Whether the underlying source analysis was complete or partial
    pub analysis_completeness: AnalysisCompleteness,
    /// Explicit warnings for skipped tables/columns or omitted stats during analysis.
    /// Each warning carries a stable `class` + `scope` so consumers can
    /// group, filter, and surface actionable hints without parsing
    /// free-text messages.
    #[serde(default)]
    pub analysis_warnings: Vec<AnalysisWarning>,
}

impl SourceAnalysisReport {
    pub fn with_analysis_warnings(mut self, warnings: Vec<AnalysisWarning>) -> Self {
        self.analysis_completeness = if warnings.is_empty() {
            AnalysisCompleteness::Complete
        } else {
            AnalysisCompleteness::Partial
        };
        self.analysis_warnings = warnings;
        self
    }

    pub fn is_partial(&self) -> bool {
        matches!(self.analysis_completeness, AnalysisCompleteness::Partial)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaStats {
    pub table_count: usize,
    pub column_count: usize,
    pub declared_fk_count: usize,
    pub total_row_count: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisCompleteness {
    Complete,
    Partial,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum WarningLevel {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisPhase {
    SchemaIntrospection,
    DataProfiling,
}

/// Stable warning classification. New backend-specific failure modes
/// are added as new variants — the discriminant survives wire format,
/// drives FE grouping, and binds to actionable hints.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum WarningClass {
    // ── Generic kernel-level outcomes ─────────────────────────────
    /// `describe_table` failed — table dropped from the analysis.
    TableSkipped,
    /// `sample_column` failed for a single column — stats omitted but
    /// the column itself is retained (type known, samples missing).
    ColumnSampleSkipped,
    /// `list_foreign_keys` failed — relationships fall back to
    /// implied-FK heuristics only.
    ForeignKeysUnavailable,
    /// Profile pass omitted distinct-value sampling for a table
    /// (rare; cardinality budget exhaustion).
    SampleValuesOmitted,

    // ── BigQuery-specific ─────────────────────────────────────────
    /// Querying the table requires a `WHERE` filter on the partition
    /// column. Hint surfaces the partition column name when known.
    BigQueryPartitionFilterRequired,
    /// Querying requires a clustering-column filter (rare; BigQuery
    /// surfaces this as a planner advisory).
    BigQueryClusteringFilterRequired,
    /// `bigquery.jobs.create` is denied on the configured billing
    /// project. Hint suggests setting `billing_project_id`.
    BigQueryJobsCreateDenied,

    // ── PostgreSQL-specific ───────────────────────────────────────
    /// Connecting role lacks `SELECT` (or sometimes `USAGE`) on the
    /// schema/table. Hint suggests granting the missing privilege.
    PostgresPermissionDenied,

    // ── Snowflake-specific ────────────────────────────────────────
    /// Configured warehouse is suspended — Snowflake auto-resumes
    /// on next query, but the introspection round-trip surfaces
    /// the wait as a warning.
    SnowflakeWarehouseSuspended,

    // ── Ontology-level drift ──────────────────────────────────────
    /// A property bound to a `ValueSetDef` carries sample values
    /// outside the set's expansion. The derived `InValueSet` rule
    /// will silently reject those values on write — operator
    /// should review the binding before the next deploy.
    /// `params.unmapped_codes` lists the offending codes;
    /// `params.value_set` names the bound set.
    ValueSetDriftDetected,
    /// A previously-introspected table's column-shape fingerprint
    /// no longer matches the live source. The shape changed (a
    /// column was added / removed / retyped / nullability flipped)
    /// or the table itself disappeared. Mappings derived against
    /// the stale shape may now disagree with the source —
    /// reviewing the binding before the next deploy keeps SHACL
    /// rules and load plans honest. `params.kind` is `"changed"`
    /// or `"removed"`.
    TableSchemaDrift,

    // ── Catch-all ─────────────────────────────────────────────────
    /// No specific class matched — the raw error is the hint.
    Other,
}

impl WarningClass {
    /// Stable, lowercase, hyphenated label for diagnostic logs and
    /// FE class names. Matches the `serde(rename_all = "snake_case")`
    /// representation but exposed without re-routing through serde.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TableSkipped => "table_skipped",
            Self::ColumnSampleSkipped => "column_sample_skipped",
            Self::ForeignKeysUnavailable => "foreign_keys_unavailable",
            Self::SampleValuesOmitted => "sample_values_omitted",
            Self::BigQueryPartitionFilterRequired => "bigquery_partition_filter_required",
            Self::BigQueryClusteringFilterRequired => "bigquery_clustering_filter_required",
            Self::BigQueryJobsCreateDenied => "bigquery_jobs_create_denied",
            Self::PostgresPermissionDenied => "postgres_permission_denied",
            Self::SnowflakeWarehouseSuspended => "snowflake_warehouse_suspended",
            Self::ValueSetDriftDetected => "value_set_drift_detected",
            Self::TableSchemaDrift => "table_schema_drift",
            Self::Other => "other",
        }
    }
}

/// Where in the source the warning originated. Structured so the FE
/// can render scoped UI (per-table sections, per-column drilldowns)
/// without parsing the free-text `location` strings the previous
/// shape carried.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WarningScope {
    /// Affects the source as a whole (e.g., FK discovery unavailable).
    Source,
    /// Affects every column of a single table.
    Table { name: String },
    /// Affects one column of one table.
    Column { table: String, column: String },
}

impl WarningScope {
    /// Compact human label used as a fallback when no FE-side
    /// localisation is in place yet.
    pub fn label(&self) -> String {
        match self {
            Self::Source => "source".to_string(),
            Self::Table { name } => name.clone(),
            Self::Column { table, column } => format!("{table}.{column}"),
        }
    }

    /// Owning table name, if the scope binds to one. `None` for
    /// `Source` warnings.
    pub fn table(&self) -> Option<&str> {
        match self {
            Self::Source => None,
            Self::Table { name } => Some(name.as_str()),
            Self::Column { table, .. } => Some(table.as_str()),
        }
    }
}

/// Single warning emitted by an adapter or the kernel during
/// analysis.
///
/// The wire shape is **language-neutral**: backend never produces
/// user-facing prose. The FE renders a localised summary and hint by
/// looking up `class` in its i18n catalogue and interpolating
/// `params` (e.g. `params.partition_column`).
///
/// `detail` carries the raw provider error for an expand-on-demand
/// drilldown — operator/debug copy, not user copy. `group_key` is
/// server-computed (`class:scope.table` or `class:source`) so the FE
/// can collapse warnings sharing a fingerprint into a single card
/// without re-implementing the grouping rule.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnalysisWarning {
    pub level: WarningLevel,
    pub phase: AnalysisPhase,
    pub class: WarningClass,
    pub scope: WarningScope,
    /// Interpolation arguments for the FE-side i18n message lookup
    /// (e.g. `{"partition_column": "stdrd_ym"}`). Keys are stable
    /// across FE locale switches; values are short identifiers, not
    /// localised prose.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub params: BTreeMap<String, String>,
    /// Raw provider error text, English. Surfaces in the
    /// expand-on-demand drilldown for operators; never the primary
    /// user-facing copy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub group_key: String,
}

impl AnalysisWarning {
    /// Deterministic group key used for FE-side fingerprinting —
    /// same `class` + same owning table coalesce into a single card.
    /// `Source`-scoped warnings share one key per class.
    pub fn group_key_for(class: WarningClass, scope: &WarningScope) -> String {
        let mut out = String::with_capacity(64);
        out.push_str(class.as_str());
        match scope.table() {
            Some(table) => {
                out.push(':');
                out.push_str(table);
            }
            None => {
                out.push_str(":source");
            }
        }
        out
    }

    /// Convenience constructor that fills `group_key` from
    /// `class` + `scope`. Use this on emit sites so adapters never
    /// hand-roll the key.
    pub fn new(
        level: WarningLevel,
        phase: AnalysisPhase,
        class: WarningClass,
        scope: WarningScope,
    ) -> Self {
        let group_key = Self::group_key_for(class, &scope);
        Self {
            level,
            phase,
            class,
            scope,
            params: BTreeMap::new(),
            detail: None,
            group_key,
        }
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn with_param(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.params.insert(key.into(), value.into());
        self
    }
}

// ---------------------------------------------------------------------------
// Implied relationships
// ---------------------------------------------------------------------------

/// A foreign key relationship inferred programmatically (not declared in schema).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpliedRelationship {
    pub from_table: String,
    pub from_column: String,
    pub to_table: String,
    pub to_column: String,
    /// 0.0–1.0 confidence (0.85 for pattern match, 0.98 if ORM-confirmed)
    pub confidence: f32,
    pub pattern: ImpliedFkPattern,
    pub reason: String,
    /// True if an ORM model or migration confirmed this relationship
    pub repo_confirmed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImpliedFkPattern {
    /// Column name ends with `_id`, stripped name matches a known table
    EntityIdSuffix,
}

// ---------------------------------------------------------------------------
// Table exclusion suggestions
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableExclusionSuggestion {
    pub table_name: String,
    pub reason: TableExclusionReason,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub row_count: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TableExclusionReason {
    /// Likely an audit / history log table
    AuditLog,
    /// Likely a temporary / migration scratch table
    Temporary,
    /// Table has zero rows — no data to model
    Empty,
}

// ---------------------------------------------------------------------------
// Large schema warning
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LargeSchemaWarning {
    pub table_count: usize,
    pub recommended_max: usize,
}

// ---------------------------------------------------------------------------
// Repo enrichment results
// ---------------------------------------------------------------------------

/// A suggestion for an ambiguous column derived from repo analysis.
/// Becomes actionable only when the user explicitly accepts it as a column_clarification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoColumnSuggestion {
    pub table: String,
    pub column: String,
    /// Suggested enum definition (e.g., "0=inactive, 1=active, 2=suspended")
    pub suggested_values: String,
    pub source_file: String,
}

/// Outcome of repo enrichment analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepoAnalysisStatus {
    /// Enrichment completed successfully
    Complete,
    /// Enrichment ran partially (e.g., some files unreadable)
    Partial,
    /// Enrichment was skipped (no relevant files found)
    Skipped,
    /// Enrichment failed (timeout, LLM error, etc.) — non-fatal, analysis continues without it
    Failed,
}

/// Structured cause of a repo-enrichment failure or skip. Replaces
/// the previous free-form `status_reason: String` so the FE can
/// render a localised "왜 실패했나" hint without parsing prose. The
/// emit site additionally `tracing::warn!`s the underlying error
/// for developer-facing observability.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepoFailureKind {
    /// Cloning a remote git URL failed (auth, network, branch missing).
    GitCloneFailed,
    /// Local repo path could not be opened.
    LocalRepoUnreadable,
    /// Repo source rejected by the workspace policy: path outside
    /// `allowed_roots` or host outside `allowed_git_hosts`. Admins
    /// resolve by extending the allow-list.
    PolicyRejected,
    /// File-tree generation produced an error (permissions, pathological symlinks).
    FileTreeFailed,
    /// LLM-driven file navigation failed (provider error, structured-output mismatch).
    LlmNavigationFailed,
    /// LLM-driven analysis call failed (provider error, schema reject).
    LlmAnalysisFailed,
    /// Provider call exceeded the configured timeout.
    Timeout,
    /// File-read pass surfaced no usable contents (binary-only, unreadable).
    NoReadableFiles,
    /// LLM-selected file list contained no relevant analysis targets.
    NoRelevantFiles,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoAnalysisSummary {
    /// Overall outcome of the repo enrichment attempt.
    pub status: RepoAnalysisStatus,
    /// Structured failure cause when `status` is `Skipped` or
    /// `Failed`. Absent on `Complete` / `Partial`. Renders to a
    /// localised hint via the FE i18n catalogue keyed by variant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_reason: Option<RepoFailureKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub framework: Option<String>,
    /// Files the LLM selected for analysis
    pub files_requested: usize,
    /// Files actually read and analyzed (may be fewer due to size/binary limits)
    pub files_analyzed: usize,
    /// Whether the file tree exceeded the max entries limit and was truncated
    pub tree_truncated: bool,
    pub enums_found: usize,
    pub relationships_found: usize,
    /// Ambiguous columns for which repo analysis found a suggestion (not yet user-accepted)
    pub columns_with_suggestions: usize,
    /// Implied FK relationships upgraded from heuristic (0.85) to ORM-confirmed (0.98) confidence
    pub fk_confidence_upgraded: usize,
    /// Git commit SHA the analysis was pinned to (present only for git URL sources)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit_sha: Option<String>,
    /// Free-form field hints from repo analysis (e.g., "ISO 4217 currency code")
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub field_hints: Vec<crate::repo_insights::FieldHint>,
    /// General domain notes from repo analysis (e.g., "multi-tenant SaaS", "soft-delete pattern")
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub domain_notes: Vec<String>,
}

// ---------------------------------------------------------------------------
// DesignOptions — user decisions passed back to the design endpoint
// ---------------------------------------------------------------------------

/// Operator-confirmed inputs that override or supplement automatic
/// analysis. Submitted via `PATCH /api/projects/:id/decisions` after
/// reviewing [`SourceAnalysisReport`].
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DesignOptions {
    /// Implied relationships the operator confirmed as real FKs.
    #[serde(default)]
    pub confirmed_relationships: Vec<ConfirmedRelationship>,
    /// PII annotations carried into ontology design. Each entry
    /// flows into the resulting [`crate::ir::PropertyDef::pii_kind`]
    /// and triggers redaction of sample values before they reach
    /// the LLM.
    #[serde(default)]
    pub pii_annotations: Vec<crate::pii::PiiAnnotation>,
    /// Source columns withheld from ontology design entirely. The
    /// LLM never sees the column's metadata or sample values.
    #[serde(default)]
    pub excluded_columns: Vec<crate::pii::ExcludedColumn>,
    /// Tables to exclude from ontology design.
    #[serde(default)]
    pub excluded_tables: Vec<String>,
    /// Free-text clarifications for ambiguous columns.
    #[serde(default)]
    pub column_clarifications: Vec<ColumnClarification>,
    /// Operator explicitly acknowledges proceeding with an incomplete
    /// source analysis (analyzer warnings present).
    #[serde(default)]
    pub partial_analysis_acknowledged: bool,
    /// Operator explicitly acknowledges designing against a schema
    /// that exceeds [`LARGE_SCHEMA_GATE_THRESHOLD`].
    #[serde(default)]
    pub large_schema_acknowledged: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfirmedRelationship {
    pub from_table: String,
    pub from_column: String,
    pub to_table: String,
    pub to_column: String,
}

/// A domain clarification for a specific column.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnClarification {
    pub table: String,
    pub column: String,
    pub hint: String,
}

// ---------------------------------------------------------------------------
// apply_pii_annotations — push annotations into the ontology IR
// ---------------------------------------------------------------------------

/// Walk every [`crate::pii::PiiAnnotation`] and set both
/// `pii_kind` and `classification` on the matching
/// [`crate::ir::PropertyDef`]. Match resolution goes through the
/// canonical [`crate::mapping::ObjectMappingDef`] list — `(table,
/// column)` from the annotation maps onto `(node, property)` via
/// the object mapping, and JSON-path locations fall back to the
/// property name when no source column was bound.
///
/// Returns the number of properties that were annotated.
pub fn apply_pii_annotations(
    ontology: &mut crate::ir::OntologyIR,
    annotations: &[crate::pii::PiiAnnotation],
    object_mappings: &[crate::mapping::ObjectMappingDef],
) -> usize {
    use crate::mapping::PropertyLocation;
    use crate::pii::data_classification_for;

    if annotations.is_empty() {
        return 0;
    }

    // (table_lower, column_lower) → kind. The most restrictive kind
    // wins when an operator submits multiple annotations on the
    // same column — restrictiveness is judged via the resulting
    // [`crate::ir::DataClassification`].
    let mut by_target: std::collections::HashMap<(String, String), crate::ir::PiiKind> =
        std::collections::HashMap::new();
    for ann in annotations {
        let key = (ann.table.to_lowercase(), ann.column.to_lowercase());
        match by_target.entry(key) {
            std::collections::hash_map::Entry::Vacant(slot) => {
                slot.insert(ann.kind.clone());
            }
            std::collections::hash_map::Entry::Occupied(mut slot) => {
                let existing_class = data_classification_for(slot.get());
                let incoming_class = data_classification_for(&ann.kind);
                if classification_strictly_above(incoming_class, existing_class) {
                    slot.insert(ann.kind.clone());
                }
            }
        }
    }

    let mut count = 0;

    for node in &mut ontology.node_types {
        let Some(om) = object_mappings
            .iter()
            .find(|om| om.node_type_id == node.id)
        else {
            continue;
        };
        let source_table = om.relation.to_lowercase();

        for prop in &mut node.properties {
            if prop.pii_kind.is_some() {
                continue;
            }
            let source_column = om
                .property_mappings
                .iter()
                .find(|pm| pm.property_id == prop.id)
                .and_then(|pm| match &pm.location {
                    PropertyLocation::Column(col) => Some(col.column.to_lowercase()),
                    PropertyLocation::JsonPath { .. } => None,
                });
            let column_lower = source_column.unwrap_or_else(|| prop.name.to_lowercase());

            if let Some(kind) = by_target.get(&(source_table.clone(), column_lower)) {
                prop.pii_kind = Some(kind.clone());
                if prop.classification.is_none() {
                    prop.classification = Some(data_classification_for(kind));
                }
                count += 1;
            }
        }
    }

    count
}

fn classification_strictly_above(
    candidate: crate::ir::DataClassification,
    current: crate::ir::DataClassification,
) -> bool {
    use crate::ir::DataClassification;
    fn rank(c: DataClassification) -> u8 {
        match c {
            DataClassification::Public => 0,
            DataClassification::Internal => 1,
            DataClassification::Confidential => 2,
            DataClassification::Restricted => 3,
        }
    }
    rank(candidate) > rank(current)
}
