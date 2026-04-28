//! `ox-ontology` — first-class ontology model for Ontosyx.
//!
//! See crate-level docs in `Cargo.toml`. This crate owns every
//! domain-model type (NodeTypeDef, EdgeTypeDef, PropertyDef, Rule,
//! Action, Function, Metric, Enrichment, Glossary, Mapping, Provenance,
//! DataQuality, Drift, Audit) — `ox-core` below it holds only the
//! primitives (`OxError`, newtypes, time).
//!
//! Phase 3-B (2026-04-20): `ontology_ir`, `ontology_input`,
//! `ontology_command`, `ontology_diff`, `load_plan`,
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
pub mod agent_view;
pub mod audit;
pub mod binding;
pub mod code_system;
pub mod command;
pub mod concept_map;
pub mod data_quality;
pub mod dependency;
pub mod derived_rules;
pub mod design_gate;
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
pub mod pii;
pub mod provenance;
pub mod quality;
pub mod repo_insights;
pub mod rule;
pub mod ambiguity;
pub mod binding_suggestions;
pub mod change_routing;
pub mod edit;
pub mod column_profile;
pub mod integrity;
pub mod notation_inference;
pub mod locale_consistency;
pub mod merge;
pub mod rule_suggestions;
pub mod segment;
pub mod source_analysis;
pub mod source_mapping;
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
pub use agent_view::{
    AgentEdgeView, AgentGlossaryView, AgentInterfaceView, AgentNodeView, AgentOntologyView,
    AgentPropertyView, AgentRuleView,
};
pub use audit::*;
pub use binding::{BindingStrength, PropertyBinding, PropertyBindingHandle};
pub use code_system::{
    CodeSystemDef, CodeSystemId, CodeSystemKind, CodedValue, CodedValueId, ucum_seed,
};
pub use concept_map::{ConceptMapDef, ConceptMapId, ConceptMapping, Equivalence, Translation};
pub use command::{
    CommandResult, EntityKind, MatchDecision, OntologyCommand, PropertyPatch,
    ReconcileConfidence, ReconcileReport, ReconcileResult, UncertainMatch,
};
pub use dependency::{
    DependencyBucket, DependencyEdge, DependencyKind, SchemaDependencyGraph, SchemaEntityRef,
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
pub use glossary::{
    GlossaryTermDef, GlossaryTermId, TaxonomyDef, TaxonomyId, TaxonomyNode, TermRelation,
    TermRelationKind,
};
pub use interface::{InterfaceDef, InterfaceEdge, InterfaceId, InterfaceProperty};
pub use diff::{
    DiffSummary, EdgeChange, EdgeDiff, NodeChange, NodeDiff, OntologyDiff, PropertyChange,
    breaking_labels, compute_diff, structural_labels,
};
pub use input::{
    InputEdgeTypeDef, InputIndexDef, InputNodeConstraint, InputNodeTypeDef, InputPropertyDef,
    NormalizeOutcome, NormalizeWarning, InputOntologyDef, normalize, to_exchange_format,
};
pub use insight::InsightHint;
pub use ir::{AggregationRole, DataClassification, OntologyIR};
pub use load_plan::{LoadMode, LoadPlan};
pub use mapping::{
    CacheHintKind, ColumnRef, EndpointRef, JoinCostHint, LinkCardinality, LinkMappingDef,
    LinkMappingId, LinkMappingKind, ObjectMappingDef, ObjectMappingId, PropertyLocation,
    PropertyMappingDef, PropertyTransform, SourceId, SourceRelationKind, SourceRelationRef,
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
    OntologyEditOp, OntologyEditPreCheck, OntologyEditReceipt, EditOntologyRequest,
    PropertyOwnerPath,
};
pub use design_gate::{
    DesignGate, GateId, GateStatus, design_allowed, evaluate_design_gates,
    unmet_blocking_gates,
};
pub use source_analysis::{
    AnalysisCompleteness, AnalysisPhase, AnalysisWarning, ColumnClarification,
    ConfirmedRelationship, DesignOptions, ImpliedFkPattern, ImpliedRelationship,
    LargeSchemaWarning, RepoAnalysisSummary, RepoColumnSuggestion, SchemaStats,
    SourceAnalysisReport, TableExclusionReason, TableExclusionSuggestion, WarningClass,
    WarningLevel, WarningScope, apply_pii_annotations,
};
pub use pii::{
    CompositePiiClassifier, ExcludedColumn, PiiAnnotation, PiiClassifier, PiiSignals,
    PiiSuggestion, REDACTED_PLACEHOLDER, RegexPiiClassifier, data_classification_for,
    redact_column_stats, redact_email, redact_last4, redact_value,
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
    TermBindingCandidate, suggest_property_bindings_by_term, suggest_terms_by_property,
};
pub use integrity::{
    DanglingKind, DanglingReference, RegistryReferenceCheck, RegistrySite,
    render_dangling_references,
};
pub use rule_suggestions::{
    RegistryChange, RuleProposal, RuleProposalTrigger, suggest_rules_for_change,
};
pub use locale_consistency::{LocaleGap, LocaleSubject, detect_locale_gaps};
pub use segment::{
    OverlapPolicy, SegmentDef, SegmentFilter, SegmentId, SegmentLiteral, SegmentRefreshPolicy,
};
pub use widget_spec::WidgetSpec;
