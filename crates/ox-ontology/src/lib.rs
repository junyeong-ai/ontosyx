//! `ox-ontology` — first-class ontology model for Ontosyx.
//!
//! Owns every domain-model type (NodeTypeDef, EdgeTypeDef,
//! PropertyDef, Rule, Action, Function, Metric, Enrichment,
//! Glossary, Mapping, Provenance, DataQuality, Drift, Audit).
//! `ox-core` sits below this crate and holds only the primitives
//! (`OxError`, newtypes, time).

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod action;
pub mod agent_view;
pub mod ambiguity;
pub mod audit;
pub mod binding;
pub mod binding_suggestions;
pub mod change_routing;
pub mod cluster_checkpoint;
pub mod code_system;
pub mod column_profile;
pub mod command;
pub mod community_detection;
pub mod concept;
pub mod concept_map;
pub mod data_quality;
pub mod dependency;
pub mod derived_rules;
pub mod design_gate;
pub mod diff;
pub mod edit;
pub mod enrichment;
pub mod eval_fingerprint;
pub mod function;
pub mod glossary;
pub mod graph_exploration;
pub mod inference_pipeline;
pub mod input;
pub mod insight;
pub mod integrity;
pub mod interface;
pub mod ir;
pub mod ir_collection;
pub mod load_plan;
pub mod locale_consistency;
pub mod mapping;
pub mod merge;
pub mod metric;
pub mod model_pricing;
pub mod notation_inference;
pub mod notation_pattern;
pub mod ontology_draft;
pub mod pii;
pub mod provenance;
pub mod quality;
pub mod rebase;
pub mod repo_insights;
pub mod retrieval;
pub mod rule;
pub mod rule_drift;
pub mod segment;
pub mod source_analysis;
pub mod source_contract;
pub mod source_mapping;
pub mod storage;
pub mod table_clustering;
pub mod table_inventory;
#[cfg(any(test, feature = "test-fixtures"))]
pub mod test_fixtures;
pub mod value_range;
pub mod value_set;
pub mod value_set_inference;
pub mod verified_query;
pub mod widget_spec;

// ---------------------------------------------------------------------------
// Re-exports — mirror the v1 call-surface so `ox_ontology::OntologyIR`
// names the headline types without the submodule hop.
// ---------------------------------------------------------------------------

pub use action::{
    ActionDef, ActionId, ActionKind, ActionTarget, ApprovalPolicy, IdempotencyPolicy, RuleId,
};
pub use agent_view::{
    AgentEdgeView, AgentGlossaryView, AgentNodeView, AgentOntologyView, AgentPropertyView,
};
pub use ambiguity::{
    AmbiguityContext, AmbiguityId, AmbiguityKind, AmbiguityMapping, AmbiguityResolution,
    AmbiguityResolutionId, RepoHint, ValueMapEntry,
};
pub use audit::*;
pub use binding::{BindingStrength, PropertyBinding, PropertyBindingHandle};
pub use binding_suggestions::{
    BindingSignal, BindingSuggestionPolicy, ConceptBindingCandidate, PropertyBindingCandidate,
    PropertyOwnerRef, suggest_concepts_by_property, suggest_property_bindings_by_term,
};
pub use change_routing::{
    ApprovalRouting, ApprovalSkipPredicate, ChangeRoutingRule, ChangeRoutingRuleId, ChangeType,
    EditContext, EditRoutingDecision, RiskLevel, RoleRef, decide_edit_routing,
};
pub use cluster_checkpoint::{ClusterSignature, DraftClusterCheckpoint};
pub use code_system::{
    CodeSystemDef, CodeSystemId, CodeSystemKind, CodedValue, CodedValueId, ucum_seed,
};
pub use command::{
    CommandResult, DeletedEntity, GeneratedEntity, MatchDecision, OntologyCommand, PreservedEntity,
    PropertyPatch, ReconcileConfidence, ReconcileEntityKind, ReconcileReport, ReconcileResult,
    UncertainMatch, build_retract_source_batch,
};
pub use concept_map::{ConceptMapDef, ConceptMapId, ConceptMapping, Equivalence, Translation};
pub use data_quality::{
    DataQualityComputationKind, DataQualityDef, DataQualityDimensionKind, DataQualityId,
    DataQualityMeasurement, DataQualityTarget,
};
pub use dependency::{
    DependencyBucket, DependencyEdge, DependencyKind, SchemaDependencyGraph, SchemaEntityRef,
};
pub use design_gate::{
    DesignGate, GateId, GateStatus, design_allowed, evaluate_design_gates, unmet_blocking_gates,
};
pub use diff::{
    DiffSummary, EdgeChange, EdgeDiff, NodeChange, NodeDiff, OntologyDiff, PropertyChange,
    breaking_labels, compute_diff, structural_labels,
};
pub use edit::{
    EditOntologyRequest, OntologyEditOp, OntologyEditPreCheck, OntologyEditReceipt,
    PropertyOwnerPath,
};
pub use enrichment::{EnrichmentDef, EnrichmentId, ExternalSourceRef, RefreshPolicy};
pub use eval_fingerprint::{
    ConfigHash, EvaluationFingerprint, EvaluationFingerprintInput, ModelId, PromptTemplateId,
    RetrievalProfileId,
};
pub use function::{
    FunctionDef, FunctionExpression, FunctionId, FunctionPurity, PropertyDependency,
};
pub use glossary::{
    GlossaryTermDef, GlossaryTermId, TaxonomyDef, TaxonomyId, TaxonomyNode, TermRealisation,
    TermRelation, TermRelationKind,
};
pub use inference_pipeline::{
    AttemptOutcome, ErrorClassification, InferenceAttempt, InferenceSession, PipelineStage,
    SessionOutcome, StageOutcome, TRANSITIONS,
};
pub use input::{
    InputEdgeTypeDef, InputIndexDef, InputNodeConstraint, InputNodeTypeDef, InputOntologyDef,
    InputPropertyDef, NormalizeOutcome, NormalizeWarning, normalize, to_exchange_format,
};
pub use insight::InsightHint;
pub use integrity::{
    DanglingKind, DanglingReference, RegistryReferenceCheck, RegistrySite,
    render_dangling_references,
};
pub use interface::{InterfaceDef, InterfaceEdge, InterfaceId, InterfaceProperty};
pub use ir::{AggregationRole, DataClassification, OntologyIR};
pub use ir_collection::IrCollection;
pub use load_plan::{LoadMode, LoadPlan};
pub use locale_consistency::{LocaleGap, LocaleSubject, detect_locale_gaps};
pub use mapping::{
    CacheHintKind, ColumnRef, EndpointRef, JoinCostHint, LinkCardinality, LinkMappingDef,
    LinkMappingId, LinkMappingKind, ObjectMappingDef, ObjectMappingId, PropertyLocation,
    PropertyMappingDef, PropertyTransform, SourceId, SourceRelationKind, SourceRelationRef,
};
pub use metric::{MetricDef, MetricExpression, MetricId, MetricScope, TemporalGrain};
pub use model_pricing::{ModelCall, ModelPrices};
pub use notation_inference::{
    NotationInferencePolicy, NotationInferenceRejection, NotationInferenceReport,
    NotationPatternProposal, NotationProposal, NotationSkip, propose_notation_pattern,
    propose_notation_patterns,
};
pub use notation_pattern::{
    NotationComponent, NotationComponentKind, NotationError, NotationPatternDef, NotationPatternId,
    ParsedComponent, ParsedValue, RenderValue,
};
pub use ontology_draft::{OntologyDraftStatus, SourceConfig, SourceHistoryEntry, SourceTypeKind};
pub use pii::{
    CompositePiiClassifier, ExcludedColumn, PiiAnnotation, PiiClassifier, PiiSignals,
    PiiSuggestion, REDACTED_PLACEHOLDER, RegexPiiClassifier, data_classification_for,
    redact_column_stats, redact_email, redact_last4, redact_value,
};
pub use provenance::{
    AgentRef, EntityRef, ProvenanceActivityKind, ProvenanceCapture, ProvenanceDef, ProvenanceId,
    ProvenancePlan, ValidationOutcomeKind,
};
pub use quality::{
    OntologyQualityReport, QualityConfidence, QualityGap, QualityGapCategory, QualityGapRef,
    QualityGapSeverity, is_cryptic_short,
};
pub use repo_insights::{
    CodeLabel, FieldHint, FileContent, FileSelection, OrmRelationType, OrmRelationship,
    RepoEnumDef, RepoInsights, ValidatedRepoSource,
};
pub use retrieval::{
    CommunityDetectionPolicy, CommunityDetectionPolicyId, RetrievalLimits, RetrievalProfile,
    TraversalStrategy,
};
pub use rule::{
    ConstraintTarget, EnforcementKind, RuleActivationKind, RuleDef, RuleKind, Severity,
    ShaclConstraint, StateTransition,
};
pub use rule_drift::detect_value_set_drift;
pub use segment::{
    OverlapPolicy, SegmentDef, SegmentFilter, SegmentId, SegmentLiteral, SegmentRefreshPolicy,
};
pub use source_analysis::{
    AnalysisCompleteness, AnalysisPhase, AnalysisWarning, ColumnClarification,
    ConfirmedRelationship, DesignOptions, ImpliedFkPattern, ImpliedRelationship,
    LargeSchemaWarning, RepoAnalysisSummary, RepoColumnSuggestion, SchemaStats,
    SourceAnalysisReport, TableExclusionReason, TableExclusionSuggestion, WarningClass,
    WarningLevel, WarningScope, apply_pii_annotations,
};
pub use source_contract::{
    ColumnSpec, SourceContractDef, SourceTypeCategory, categorize_data_type,
};
pub use table_clustering::{ClusterPlan, TableCluster, cluster_tables};
pub use table_inventory::{TableInventoryEntry, TableInventoryStatus};
pub use value_range::{ValueBand, ValueRangeSetDef, ValueRangeSetId};
pub use value_set::{
    ExpandWarning, ExpandedValueSet, IncludeMode, ValueSetDef, ValueSetId, ValueSetIncludeRule,
    ValueSetSelector, expand_value_set,
};
pub use value_set_inference::{
    ValueSetEvidence, ValueSetInferencePolicy, ValueSetInferenceReport, ValueSetProposal,
    ValueSetRejection, ValueSetSkip, propose_value_sets,
};
pub use verified_query::{
    ComplexityClass, VerifiedQueryDef, VerifiedQueryId, VerifiedQueryStatus,
    canonicalize_question, question_hash,
};
pub use widget_spec::WidgetSpec;
