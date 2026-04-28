#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unreachable
    )
)]

pub mod models;
pub mod navigation;
pub mod postgres;
pub mod quality_signal;
pub mod secret_token;
pub mod store;

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
pub use postgres::{SYSTEM_BYPASS, WORKSPACE_ID};
pub use store::{
    AclStore, AgentSessionStore, AmbiguityStore, AnalysisResultStore, AnalysisSnapshot,
    ApiKeyStore, ApprovalCommentStore, ApprovalStore, AuditRecord, AuditStore, AuditTrailFilter,
    AuditTrailStore, ChangeRoutingStore, ConfigStore,
    CursorPage, CursorParams, DashboardStore, DataSourceStore, EmbeddingRetryStore, ExtendResult,
    HealthStore, InsightStore, KnowledgeStore, LineageStore, LoadCheckpointStore, MeteringStore, ModelConfigStore,
    NotificationStore, OntologyNavigationStore, OntologyVersionStore, PatternStore,
    PerspectiveStore, PinStore, ProjectStore, PromptTemplateStore, QualityBaselineStore,
    QualitySignalStore, QualityStore, QueryStore, RecipeStore, ReportStore, ScheduledTaskStore,
    StaleConceptProposalStore, Store, ToolApprovalStore, UserStore, VerificationStore,
    WorkspaceStore,
};
