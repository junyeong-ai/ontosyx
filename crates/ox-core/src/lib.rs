#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unreachable
    )
)]

//! `ox-core` — DB-agnostic IR and support types for the Ontosyx platform.
//!
//! This crate is the single source of truth for every type that flows
//! between the compiler, runtime, brain, agent, store, and web layers.
//! It has no heavy dependencies — every other crate depends on this one,
//! and nothing here depends on a graph DB driver, an HTTP server, or an
//! LLM client.
//!
//! The public surface is grouped into three families so callers can tell
//! at a glance which layer of the pipeline a given type belongs to:
//!
//! 1. **Runtime / compile-target IRs.** The shapes the compiler and
//!    runtime consume. `OntologyIR`, `QueryIR`, `PatternIR`, plus the
//!    `StructuredMatchQuery` wire-format for LLM-generated match queries.
//!    Immutable after construction; schema-versioned via
//!    `ONTOLOGY_IR_SCHEMA_VERSION` / `QUERY_IR_SCHEMA_VERSION` /
//!    `PATTERN_IR_SCHEMA_VERSION`.
//! 2. **Input DTOs.** Shapes produced by user / LLM input *before*
//!    validation produces a runtime IR. `OntologyInputIR` and friends
//!    (`Input*Def`) land here; `normalize()` turns them into an
//!    `OntologyIR`.
//! 3. **Analysis outputs.** Shapes the source-introspection and
//!    repo-analysis pipelines emit. `SourceAnalysisReport`,
//!    `OntologyQualityReport`, `RepoInsights`, etc. Consumed by the
//!    design-project flow and by the UI.
//!
//! Infrastructure types (`OxError`, `LocalizedText`, `PromptVersion`, ...)
//! live at the crate root and are re-exported under an "Infrastructure"
//! section below.

/// Shared test fixtures, including the Korean e-commerce golden ontology.
///
/// Always compiled in `cfg(test)` for intra-crate tests. Downstream crates
/// must opt in via the `test-fixtures` feature to use these helpers in
/// their own integration or unit tests.
#[cfg(any(test, feature = "test-fixtures"))]
pub mod test_fixtures;

// ---------------------------------------------------------------------------
// Module declarations — grouped by role for skimmability; the re-export
// section below mirrors the same groups so a new reader can see at a
// glance which layer of the pipeline a type belongs to.
// ---------------------------------------------------------------------------

// Infrastructure
pub mod error;
pub mod i18n;
pub mod prompt_version;
pub mod types;

// Runtime / compile-target IRs
pub mod ontology_ir;
pub mod pattern_ir;
pub mod query_bindings;
pub mod query_ir;
pub mod structured_match_query;

// Input DTOs (user / LLM input before validation)
pub mod ontology_command;
pub mod ontology_diff;
pub mod ontology_input;
pub mod load_plan;

// Analysis outputs (source-introspection, repo analysis, quality)
pub mod design_project;
pub mod eval;
pub mod graph_audit;
pub mod graph_exploration;
pub mod quality;
pub mod repo_insights;
pub mod source_analysis;
pub mod source_mapping;
pub mod source_schema;
pub mod table_clustering;
pub mod widget_spec;

// ---------------------------------------------------------------------------
// Re-exports — Infrastructure
// ---------------------------------------------------------------------------

pub use error::{ErrorContext, OxError};
pub use i18n::{LanguageTag, LocaleError, LocalizedText};
pub use prompt_version::PromptVersion;
pub use types::{escape_cypher_identifier, is_valid_graph_identifier, sanitize_variable};

// ---------------------------------------------------------------------------
// Re-exports — Runtime / compile-target IRs
// ---------------------------------------------------------------------------

pub use ontology_ir::{DataClassification, OntologyIR};
pub use pattern_ir::{
    LayoutHints, PatternEdge, PatternFilter, PatternIR, PatternNode, PatternProjection, Position,
};
pub use query_bindings::{
    BindingKind, EdgeBinding, NodeBinding, PropertyBinding, ResolvedQueryBindings,
    resolve_query_bindings,
};
pub use query_ir::QueryIR;
pub use structured_match_query::StructuredMatchQuery;

// ---------------------------------------------------------------------------
// Re-exports — Input DTOs (user / LLM input before validation)
// ---------------------------------------------------------------------------

pub use load_plan::{LoadMode, LoadPlan};
pub use ontology_command::{
    CommandResult, EntityKind, MatchDecision, OntologyCommand, PropertyPatch, ReconcileConfidence,
    ReconcileReport, ReconcileResult, UncertainMatch,
};
pub use ontology_diff::{
    DiffSummary, EdgeChange, EdgeDiff, NodeChange, NodeDiff, OntologyDiff, PropertyChange,
    breaking_labels, compute_diff, structural_labels,
};
pub use ontology_input::{
    InputEdgeTypeDef, InputIndexDef, InputNodeConstraint, InputNodeTypeDef, InputPropertyDef,
    NormalizeResult, NormalizeWarning, OntologyInputIR, normalize, to_exchange_format,
};

// ---------------------------------------------------------------------------
// Re-exports — Analysis outputs (source-introspection, repo analysis, quality)
// ---------------------------------------------------------------------------

pub use design_project::{DesignProjectStatus, SourceConfig, SourceHistoryEntry, SourceTypeKind};
pub use quality::{
    OntologyQualityReport, QualityConfidence, QualityGap, QualityGapCategory, QualityGapRef,
    QualityGapSeverity, is_cryptic_short,
};
pub use repo_insights::{
    CodeLabel, FieldHint, FileContent, FileSelection, OrmRelationType, OrmRelationship,
    RepoEnumDef, RepoInsights, ValidatedRepoSource,
};
pub use source_analysis::{
    AmbiguityType, AmbiguousColumn, AnalysisCompleteness, AnalysisPhase, AnalysisWarning,
    AnalysisWarningKind, ColumnClarification, ConfirmedRelationship, DesignOptions,
    ImpliedFkPattern, ImpliedRelationship, LargeSchemaWarning, PiiDecision, PiiDecisionEntry,
    PiiFinding, RepoAnalysisSummary, RepoColumnSuggestion, SchemaStats, SourceAnalysisReport,
    TableExclusionReason, TableExclusionSuggestion, WarningLevel, apply_pii_classifications,
};
pub use source_mapping::SourceMapping;
pub use source_schema::{SourceProfile, SourceSchema};
pub use table_clustering::{ClusterPlan, TableCluster, cluster_tables};
pub use widget_spec::WidgetSpec;

// ---------------------------------------------------------------------------
// InsightSuggestion — proactive insight generated from ontology structure
// ---------------------------------------------------------------------------

/// A proactive insight suggestion generated from ontology structure.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct InsightSuggestion {
    /// Natural language question a data analyst would ask
    pub question: String,
    /// Category: "trend", "distribution", "anomaly", "relationship", "summary"
    pub category: String,
    /// Suggested tool: "query_graph" or "execute_analysis"
    pub suggested_tool: String,
}
