#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unreachable
    )
)]

pub mod advisory_lock;
pub mod community;
pub mod evaluation;
pub mod inference;
pub mod models;
pub mod navigation;
pub mod postgres;
pub mod quality_signal;
pub mod secret_token;
pub mod store;

pub use evaluation::{
    EvaluationCapture, EvaluationCase, EvaluationContext, EvaluationMetric, EvaluationRun,
    EvaluationRunStatus, NullEvaluationCapture, current_evaluation_context, parse_run_status,
    scope_evaluation_context,
};
pub use inference::{
    InferenceContext, current_inference_context, run_in_inference_session,
    scope_inference_context,
};
pub use models::*;
pub use navigation::{
    BlendWeights, EntityRef, EntitySearchHit, EntryPointSearchOptions, FacetFilter,
    HierarchyExpand, HierarchyFacetOptions, LlmRenderOptions, NeighborDirection,
    NeighborExpandOptions, Subgraph, SubgraphEdge, SubgraphNode,
};
pub use postgres::PostgresStore;
pub use postgres::{SYSTEM_BYPASS, WORKSPACE_ID, require_workspace_context};
pub use quality_signal::{
    MetricValue, MetricWindow, QualityMetricsReport, QueryExecutionSignal, ShaclFailureCount,
    ShaclFailureKind, StaleConceptProposal, StaleProposalDecision, StaleTypeEntry,
    WorkspaceQualityBaseline,
};
// Re-export `PgPool` so downstream crates that wire singleton
// crons (`ox-api::background`) can refer to the type without
// taking `sqlx` as a direct dependency.
pub use sqlx::PgPool;
pub use store::{
    AclStore, AgentSessionStore, AmbiguityStore, AnalysisSnapshot, ApiKeyStore,
    ApprovalCommentStore, ApprovalStore, AuditRecord, AuditStore, AuditTrailFilter,
    AuditTrailStore, ChangeRoutingStore, CommunityDetectionPolicyStore, CommunitySummaryStore,
    ConfigStore, CursorPage,
    CursorParams, DashboardStore, DataSourceStore, DraftClusterCheckpointStore,
    EmbeddingRetryStore, EvaluationStore, ExtendResult, HealthStore, IdempotencyStore,
    InferenceSessionStore, InsightStore, JwtRevocationStore, KnowledgeStore, LineageStore,
    LoadCheckpointStore,
    MeteringStore, ModelConfigStore, NotificationStore, OntologyDraftStore,
    OntologyNavigationStore, OntologyVersionStore, PatternStore, PerspectiveStore, PinStore,
    PromptTemplateStore, ProvenanceStore, QualityBaselineStore, QualitySignalStore, QualityStore,
    QueryStore, RecipeExecutionStore, RecipeStore, ReportStore, RetrievalProfileStore,
    ScheduledTaskStore, SourceContractStore,
    SourceMappingArtifactStore, StaleConceptProposalStore, Store, ToolApprovalStore, UserStore,
    VerificationStore, VerifiedQueryStore, WorkspaceStore,
};
