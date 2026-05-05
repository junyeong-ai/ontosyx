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
pub mod evaluation;
pub mod models;
pub mod navigation;
pub mod postgres;
pub mod quality_signal;
pub mod secret_token;
pub mod store;

pub use evaluation::{
    parse_run_status, EvaluationCase, EvaluationMetric, EvaluationRun, EvaluationRunStatus,
};
pub use models::*;
pub use navigation::{
    BlendWeights, EntityRef, EntitySearchHit, EntryPointSearchOptions, FacetFilter,
    HierarchyExpand, HierarchyFacetOptions, LlmRenderOptions, NeighborDirection,
    NeighborExpandOptions, Subgraph, SubgraphEdge, SubgraphNode,
};
pub use quality_signal::{
    MetricValue, MetricWindow, QualityMetricsReport, QueryExecutionSignal, ShaclFailureCount,
    ShaclFailureKind, StaleConceptProposal, StaleProposalDecision, StaleTypeEntry,
    WorkspaceQualityBaseline,
};
pub use postgres::PostgresStore;
pub use postgres::{SYSTEM_BYPASS, WORKSPACE_ID, require_workspace_context};
// Re-export `PgPool` so downstream crates that wire singleton
// crons (`ox-api::background`) can refer to the type without
// taking `sqlx` as a direct dependency.
pub use sqlx::PgPool;
pub use store::{
    AclStore, AgentSessionStore, AmbiguityStore, AnalysisResultStore, AnalysisSnapshot,
    ApiKeyStore, ApprovalCommentStore, ApprovalStore, AuditRecord, AuditStore, AuditTrailFilter,
    AuditTrailStore, ChangeRoutingStore, ConfigStore, CursorPage, CursorParams, DashboardStore,
    DataSourceStore, DraftClusterCheckpointStore, EmbeddingRetryStore, EvaluationStore,
    ExtendResult, HealthStore, IdempotencyStore, InsightStore, JwtRevocationStore, KnowledgeStore,
    LineageStore, LoadCheckpointStore, MeteringStore, ModelConfigStore, NotificationStore,
    OntologyNavigationStore, OntologyVersionStore, PatternStore, PerspectiveStore, PinStore,
    OntologyDraftStore, PromptTemplateStore, QualityBaselineStore, QualitySignalStore, QualityStore,
    QueryStore, RecipeStore, ReportStore, ScheduledTaskStore, SourceMappingArtifactStore,
    StaleConceptProposalStore, Store, ToolApprovalStore, UserStore, VerificationStore,
    WorkspaceStore,
};
