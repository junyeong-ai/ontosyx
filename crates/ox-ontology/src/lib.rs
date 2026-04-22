//! `ox-ontology` — first-class ontology model for Ontosyx.
//!
//! See crate-level docs in `Cargo.toml`. This crate owns every
//! domain-model type (NodeTypeDef, EdgeTypeDef, PropertyDef, Rule,
//! Action, Function, Metric, Enrichment, Glossary, Mapping, Provenance,
//! DataQuality, Drift, Audit) — `ox-core` below it holds only the
//! primitives (`OxError`, newtypes, time).
//!
//! Phase 3-B (2026-04-20): `ontology_ir`, `ontology_input`,
//! `ontology_command`, `ontology_diff`, `source_mapping`, `load_plan`,
//! `graph_audit`, `source_analysis`, `quality`, `test_fixtures`,
//! `table_clustering`, `design_project`, `widget_spec`,
//! `repo_insights`, `graph_exploration` migrated here from
//! `ox-core`, renamed to `ir`, `input`, `command`, `diff`, `mapping`,
//! `load_plan`, `audit` (and keep their names for the rest).

#![cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

pub mod action;
pub mod audit;
pub mod code_system;
pub mod command;
pub mod concept_map;
pub mod data_quality;
pub mod design_project;
pub mod diff;
pub mod enrichment;
pub mod function;
pub mod glossary;
pub mod graph_exploration;
pub mod input;
pub mod insight;
pub mod interface;
pub mod ir;
pub mod load_plan;
pub mod mapping;
pub mod metric;
pub mod notation_pattern;
pub mod provenance;
pub mod quality;
pub mod repo_insights;
pub mod rule;
pub mod ambiguity;
pub mod binding_suggestions;
pub mod change_routing;
pub mod edit;
pub mod integrity;
pub mod locale_consistency;
pub mod rule_suggestions;
pub mod source_analysis;
pub mod storage;
pub mod table_clustering;
#[cfg(any(test, feature = "test-fixtures"))]
pub mod test_fixtures;
pub mod value_range;
pub mod value_set;
pub mod value_set_inference;
pub mod widget_spec;

// ---------------------------------------------------------------------------
// Re-exports — mirror the v1 call-surface so `ox_ontology::OntologyIR`
// names the headline types without the submodule hop.
// ---------------------------------------------------------------------------

pub use action::{
    ActionDef, ActionId, ActionKind, ActionTarget, ApprovalPolicy, IdempotencyPolicy, RuleId,
};
pub use audit::*;
pub use code_system::{
    CodeSystemDef, CodeSystemId, CodeSystemKind, CodedValue, CodedValueId, ucum_seed,
};
pub use concept_map::{ConceptMapDef, ConceptMapId, ConceptMapping, Equivalence, Translation};
pub use command::{
    CommandResult, EntityKind, MatchDecision, OntologyCommand, PropertyPatch,
    ReconcileConfidence, ReconcileReport, ReconcileResult, UncertainMatch,
};
pub use data_quality::{
    DataQualityComputationKind, DataQualityDef, DataQualityDimensionKind, DataQualityId,
    DataQualityMeasurement, DataQualityTarget,
};
pub use design_project::{DesignProjectStatus, SourceConfig, SourceHistoryEntry, SourceTypeKind};
pub use enrichment::{EnrichmentDef, EnrichmentId, ExternalSourceRef, RefreshPolicy};
pub use function::{
    FunctionDef, FunctionExpression, FunctionId, FunctionPurity, PropertyDependency,
};
pub use glossary::{GlossaryTermDef, GlossaryTermId, TaxonomyDef, TaxonomyId, TaxonomyNode};
pub use interface::{InterfaceDef, InterfaceEdge, InterfaceId, InterfaceProperty};
pub use diff::{
    DiffSummary, EdgeChange, EdgeDiff, NodeChange, NodeDiff, OntologyDiff, PropertyChange,
    breaking_labels, compute_diff, structural_labels,
};
pub use input::{
    InputEdgeTypeDef, InputIndexDef, InputNodeConstraint, InputNodeTypeDef, InputPropertyDef,
    NormalizeResult, NormalizeWarning, InputOntologyDef, normalize, to_exchange_format,
};
pub use insight::InsightSuggestion;
pub use ir::{AggregationRole, DataClassification, OntologyIR};
pub use load_plan::{LoadMode, LoadPlan};
pub use mapping::{
    CacheHintKind, ColumnRef, EndpointRef, JoinCostHint, LinkCardinality, LinkMappingDef,
    LinkMappingId, LinkMappingKind, ObjectMappingDef, ObjectMappingId, PropertyLocation,
    PropertyMappingDef, PropertyTransform, SourceId, SourceMapping, SourceRelationKind,
    SourceRelationRef,
};
pub use metric::{MetricDef, MetricExpression, MetricId, MetricScope, TemporalGrain};
pub use notation_pattern::{
    NotationComponent, NotationComponentKind, NotationError, NotationPatternDef,
    NotationPatternId, ParsedComponent, ParsedValue, RenderValue,
};
pub use provenance::{
    AgentRef, EntityRef, ProvenanceActivityKind, ProvenanceDef, ProvenanceId,
    ValidationOutcomeKind,
};
pub use rule::{
    ConstraintTarget, EnforcementKind, RuleActivationKind, RuleDef, RuleKind, Severity,
    ShaclConstraint, StateTransition,
};
pub use quality::{
    OntologyQualityReport, QualityConfidence, QualityGap, QualityGapCategory, QualityGapRef,
    QualityGapSeverity, is_cryptic_short,
};
pub use repo_insights::{
    CodeLabel, FieldHint, FileContent, FileSelection, OrmRelationType, OrmRelationship,
    RepoEnumDef, RepoInsights, ValidatedRepoSource,
};
pub use ambiguity::{
    AmbiguityContext, AmbiguityId, AmbiguityKind, AmbiguityMapping, AmbiguityResolution,
    AmbiguityResolutionId, RepoHint, ValueMapEntry,
};
pub use change_routing::{
    ApprovalRouting, ApprovalSkipPredicate, ChangeRoutingRule, ChangeRoutingRuleId, ChangeType,
    EditContext, EditRoutingDecision, RiskLevel, RoleRef, decide_edit_routing,
};
pub use edit::{
    OntologyEditOp, OntologyEditPreCheck, OntologyEditReceipt, OntologyEditRequest,
};
pub use source_analysis::{
    AnalysisCompleteness, AnalysisPhase, AnalysisWarning, AnalysisWarningKind,
    ColumnClarification, ConfirmedRelationship, DesignOptions, ImpliedFkPattern,
    ImpliedRelationship, LargeSchemaWarning, PiiDecision, PiiDecisionEntry, PiiFinding,
    RepoAnalysisSummary, RepoColumnSuggestion, SchemaStats, SourceAnalysisReport,
    TableExclusionReason, TableExclusionSuggestion, WarningLevel, apply_pii_classifications,
};
pub use table_clustering::{ClusterPlan, TableCluster, cluster_tables};
pub use value_range::{ValueBand, ValueRangeSetDef, ValueRangeSetId};
pub use value_set::{
    ExpandWarning, ExpandedValueSet, IncludeMode, ValueSetDef, ValueSetId, ValueSetIncludeRule,
    ValueSetSelector, expand_value_set,
};
pub use value_set_inference::{
    ValueSetEvidence, ValueSetInferencePolicy, ValueSetInferenceReport, ValueSetProposal,
    ValueSetRejection, ValueSetSkip, propose_value_sets,
};
pub use binding_suggestions::{
    BindingSignal, BindingSuggestionPolicy, PropertyBindingCandidate, PropertyOwnerRef,
    TermBindingCandidate, suggest_property_bindings_for_term, suggest_terms_for_property,
};
pub use integrity::{
    DanglingKind, DanglingReference, RegistryReferenceCheck, RegistrySite,
    render_dangling_references,
};
pub use rule_suggestions::{
    RegistryChange, RuleProposal, RuleProposalTrigger, suggest_rules_for_change,
};
pub use locale_consistency::{LocaleGap, LocaleSubject, detect_locale_gaps};
pub use widget_spec::WidgetSpec;
