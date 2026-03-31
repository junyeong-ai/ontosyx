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
    /// Columns that likely contain personal identifiable information
    pub pii_findings: Vec<PiiFinding>,
    /// Columns whose values are ambiguous and need user clarification
    pub ambiguous_columns: Vec<AmbiguousColumn>,
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
    /// Explicit warnings for skipped tables/columns or omitted stats during analysis
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnalysisWarning {
    pub level: WarningLevel,
    pub phase: AnalysisPhase,
    pub kind: AnalysisWarningKind,
    pub location: String,
    pub message: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisPhase {
    SchemaIntrospection,
    DataProfiling,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisWarningKind {
    TableSkipped,
    ColumnSkipped,
    ForeignKeysUnavailable,
    SampleValuesOmitted,
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
// PII detection
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiiFinding {
    pub table: String,
    pub column: String,
    pub pii_type: PiiType,
    pub detection_method: PiiDetectionMethod,
    /// Masked preview (e.g., "hong**@***.com") shown in the report UI
    #[serde(skip_serializing_if = "Option::is_none")]
    pub masked_preview: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PiiType {
    Name,
    Email,
    Phone,
    BirthDate,
    NationalId,
    Address,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PiiDetectionMethod {
    /// Column name contains a PII keyword (e.g., "email", "phone")
    ColumnName,
    /// Sample value matches a PII pattern (e.g., contains '@')
    ValuePattern,
}

// ---------------------------------------------------------------------------
// Ambiguous columns
// ---------------------------------------------------------------------------

/// A column whose values cannot be interpreted from schema alone.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AmbiguousColumn {
    pub table: String,
    pub column: String,
    pub ambiguity_type: AmbiguityType,
    pub sample_values: Vec<String>,
    /// Suggested question to ask the user
    pub clarification_prompt: String,
    /// Pre-filled suggestion from repo analysis (e.g., "0=inactive, 1=active").
    /// Present only when repo enrichment found a matching enum definition.
    /// The user must explicitly accept, edit, or reject this suggestion.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo_suggestion: Option<RepoSuggestion>,
}

/// A suggestion derived from repo analysis for an ambiguous column.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoSuggestion {
    /// Suggested values (e.g., "0=inactive, 1=active, 2=suspended")
    pub suggested_values: String,
    /// Source file where the definition was found
    pub source_file: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AmbiguityType {
    /// All values are integers (e.g., 1, 2, 3) — likely status/type codes
    NumericCode,
    /// Short uppercase codes mixed with longer meaningful strings
    OpaqueShortCode,
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
    pub suggestion: String,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoAnalysisSummary {
    /// Overall outcome of the repo enrichment attempt
    pub status: RepoAnalysisStatus,
    /// Human-readable reason when status is skipped or failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_reason: Option<String>,
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

/// User-approved decisions that override or supplement automatic analysis.
/// Submitted via `PATCH /api/projects/:id/decisions` after reviewing SourceAnalysisReport.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DesignOptions {
    /// Implied relationships the user confirmed as real FKs
    #[serde(default)]
    pub confirmed_relationships: Vec<ConfirmedRelationship>,
    /// Per-column PII handling decisions
    #[serde(default)]
    pub pii_decisions: Vec<PiiDecisionEntry>,
    /// Tables to exclude from ontology design
    #[serde(default)]
    pub excluded_tables: Vec<String>,
    /// Free-text clarifications for ambiguous columns
    #[serde(default)]
    pub column_clarifications: Vec<ColumnClarification>,
    /// User explicitly accepts proceeding with incomplete source analysis.
    #[serde(default)]
    pub allow_partial_source_analysis: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfirmedRelationship {
    pub from_table: String,
    pub from_column: String,
    pub to_table: String,
    pub to_column: String,
}

/// A PII handling decision for a specific column.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiiDecisionEntry {
    pub table: String,
    pub column: String,
    pub decision: PiiDecision,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PiiDecision {
    /// Replace sample values with "[MASKED]" before sending to LLM
    Mask,
    /// Exclude this column entirely from the ontology
    Exclude,
    /// Allow as-is (user confirms it's acceptable)
    Allow,
}

/// A domain clarification for a specific column.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnClarification {
    pub table: String,
    pub column: String,
    pub hint: String,
}
