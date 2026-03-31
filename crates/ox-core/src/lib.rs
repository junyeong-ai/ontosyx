#[cfg(test)]
pub(crate) mod test_fixtures;

pub mod eval;

pub mod design_project;
pub mod error;
pub mod graph_audit;
pub mod graph_exploration;
pub mod load_plan;
pub mod ontology_command;
pub mod ontology_diff;
pub mod ontology_input;
pub mod ontology_ir;
pub mod quality;
pub mod query_bindings;
pub mod query_ir;
pub mod repo_insights;
pub mod source_analysis;
pub mod source_mapping;
pub mod source_schema;
pub mod table_clustering;
pub mod types;
pub mod widget_spec;

pub use design_project::{DesignProjectStatus, SourceConfig, SourceHistoryEntry, SourceTypeKind};
pub use error::{ErrorContext, OxError};
pub use load_plan::LoadPlan;
pub use ontology_command::{
    CommandResult, EntityKind, MatchDecision, OntologyCommand, PropertyPatch, ReconcileConfidence,
    ReconcileReport, ReconcileResult, UncertainMatch,
};
pub use ontology_input::{
    InputEdgeTypeDef, InputIndexDef, InputNodeConstraint, InputNodeTypeDef, InputPropertyDef,
    NormalizeResult, NormalizeWarning, OntologyInputIR, normalize, to_exchange_format,
};
pub use ontology_diff::{
    DiffSummary, EdgeChange, EdgeDiff, NodeChange, NodeDiff, OntologyDiff, PropertyChange,
    compute_diff,
};
pub use ontology_ir::OntologyIR;
pub use quality::{
    OntologyQualityReport, QualityConfidence, QualityGap, QualityGapCategory, QualityGapRef,
    QualityGapSeverity, is_cryptic_short,
};
pub use query_bindings::{
    BindingKind, EdgeBinding, NodeBinding, PropertyBinding, ResolvedQueryBindings,
    resolve_query_bindings,
};
pub use query_ir::QueryIR;
pub use repo_insights::{
    CodeLabel, FieldHint, FileContent, FileSelection, OrmRelationType, OrmRelationship,
    RepoEnumDef, RepoInsights, ValidatedRepoSource,
};
pub use source_analysis::{
    AmbiguityType, AmbiguousColumn, AnalysisCompleteness, AnalysisPhase, AnalysisWarning,
    AnalysisWarningKind, ColumnClarification, ConfirmedRelationship, DesignOptions,
    ImpliedFkPattern, ImpliedRelationship, LargeSchemaWarning, PiiDecision, PiiDecisionEntry,
    PiiFinding, RepoAnalysisSummary, RepoColumnSuggestion, SchemaStats, SourceAnalysisReport,
    TableExclusionReason, TableExclusionSuggestion, WarningLevel,
};
pub use source_mapping::SourceMapping;
pub use source_schema::{SourceProfile, SourceSchema};
pub use table_clustering::{ClusterPlan, TableCluster, cluster_tables};
pub use types::{escape_cypher_identifier, is_valid_graph_identifier};
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
