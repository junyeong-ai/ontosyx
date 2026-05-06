//! Trait-segregated store interface.
//!
//! Each focus area lives in its own submodule (one trait per file
//! mirroring `postgres/` impl layout). The [`Store`] supertrait
//! composes them so consumers carry a single trait bound; the
//! blanket impl auto-derives membership for any concrete struct
//! that implements every sub-trait.
//!
//! Shared types ([`CursorParams`] / [`CursorPage`] for pagination,
//! [`AnalysisSnapshot`] / [`ExtendResult`] for the
//! [`OntologyDraftStore`] grouped-parameter shapes) live alongside
//! the supertrait so every domain module can reach them through
//! `super::`.

mod cursor;
mod snapshot;

mod acl;
mod agent_session;
mod ambiguity;
mod api_key;
mod approval;
mod approval_comment;
mod audit;
mod audit_trail;
mod change_routing;
mod config;
mod dashboard;
mod data_source;
mod draft_cluster_checkpoint;
mod embedding_retry;
mod evaluation;
mod health;
mod idempotency;
mod insight;
mod jwt_revocation;
mod knowledge;
mod lineage;
mod load_checkpoint;
mod metering;
mod model_config;
mod notification;
mod ontology_draft;
mod ontology_navigation;
mod ontology_version;
mod pattern;
mod perspective;
mod pin;
mod prompt_template;
mod quality;
mod quality_baseline;
mod quality_signal;
mod query;
mod recipe;
mod recipe_execution;
mod report;
mod scheduled_task;
mod source_mapping_artifact;
mod stale_concept_proposal;
mod tool_approval;
mod user;
mod verification;
mod workspace;

pub use cursor::{CursorPage, CursorParams};
pub use snapshot::{AnalysisSnapshot, ExtendResult};

pub use acl::AclStore;
pub use agent_session::AgentSessionStore;
pub use ambiguity::AmbiguityStore;
pub use api_key::ApiKeyStore;
pub use approval::ApprovalStore;
pub use approval_comment::ApprovalCommentStore;
pub use audit::AuditStore;
pub use audit_trail::{AuditRecord, AuditTrailFilter, AuditTrailStore};
pub use change_routing::ChangeRoutingStore;
pub use config::ConfigStore;
pub use dashboard::DashboardStore;
pub use data_source::DataSourceStore;
pub use draft_cluster_checkpoint::DraftClusterCheckpointStore;
pub use embedding_retry::EmbeddingRetryStore;
pub use evaluation::EvaluationStore;
pub use health::HealthStore;
pub use idempotency::IdempotencyStore;
pub use insight::{CreateInsightInput, InsightFilter, InsightStore, UpdateInsightInput};
pub use jwt_revocation::JwtRevocationStore;
pub use knowledge::KnowledgeStore;
pub use lineage::LineageStore;
pub use load_checkpoint::LoadCheckpointStore;
pub use metering::MeteringStore;
pub use model_config::ModelConfigStore;
pub use notification::NotificationStore;
pub use ontology_draft::OntologyDraftStore;
pub use ontology_navigation::OntologyNavigationStore;
pub use ontology_version::OntologyVersionStore;
pub use pattern::PatternStore;
pub use perspective::PerspectiveStore;
pub use pin::PinStore;
pub use prompt_template::PromptTemplateStore;
pub use quality::QualityStore;
pub use quality_baseline::QualityBaselineStore;
pub use quality_signal::QualitySignalStore;
pub use query::QueryStore;
pub use recipe::RecipeStore;
pub use recipe_execution::RecipeExecutionStore;
pub use report::ReportStore;
pub use scheduled_task::ScheduledTaskStore;
pub use source_mapping_artifact::SourceMappingArtifactStore;
pub use stale_concept_proposal::StaleConceptProposalStore;
pub use tool_approval::ToolApprovalStore;
pub use user::UserStore;
pub use verification::VerificationStore;
pub use workspace::WorkspaceStore;

/// Aggregating supertrait. Any concrete persistence backend that
/// implements every sub-trait gets a free `Store` impl through the
/// blanket below — consumers can take `Arc<dyn Store>` without each
/// one re-spelling the full bound list.
pub trait Store:
    AclStore
    + AgentSessionStore
    + AmbiguityStore
    + ApiKeyStore
    + ApprovalStore
    + ApprovalCommentStore
    + AuditStore
    + AuditTrailStore
    + ChangeRoutingStore
    + ConfigStore
    + DashboardStore
    + DataSourceStore
    + DraftClusterCheckpointStore
    + EmbeddingRetryStore
    + EvaluationStore
    + HealthStore
    + IdempotencyStore
    + InsightStore
    + JwtRevocationStore
    + KnowledgeStore
    + LineageStore
    + LoadCheckpointStore
    + MeteringStore
    + ModelConfigStore
    + NotificationStore
    + OntologyDraftStore
    + OntologyNavigationStore
    + OntologyVersionStore
    + PatternStore
    + PerspectiveStore
    + PinStore
    + PromptTemplateStore
    + QualityStore
    + QualityBaselineStore
    + QualitySignalStore
    + QueryStore
    + RecipeStore
    + RecipeExecutionStore
    + ReportStore
    + ScheduledTaskStore
    + SourceMappingArtifactStore
    + StaleConceptProposalStore
    + ToolApprovalStore
    + UserStore
    + VerificationStore
    + WorkspaceStore
{
}

impl<T> Store for T where
    T: AclStore
        + AgentSessionStore
        + AmbiguityStore
        + ApiKeyStore
        + ApprovalStore
        + ApprovalCommentStore
        + AuditStore
        + AuditTrailStore
        + ChangeRoutingStore
        + ConfigStore
        + DashboardStore
        + DataSourceStore
        + DraftClusterCheckpointStore
        + EmbeddingRetryStore
        + EvaluationStore
        + HealthStore
        + IdempotencyStore
        + InsightStore
        + JwtRevocationStore
        + KnowledgeStore
        + LineageStore
        + LoadCheckpointStore
        + MeteringStore
        + ModelConfigStore
        + NotificationStore
        + OntologyDraftStore
        + OntologyNavigationStore
        + OntologyVersionStore
        + PatternStore
        + PerspectiveStore
        + PinStore
        + PromptTemplateStore
        + QualityStore
        + QualityBaselineStore
        + QualitySignalStore
        + QueryStore
        + RecipeStore
        + RecipeExecutionStore
        + ReportStore
        + ScheduledTaskStore
        + SourceMappingArtifactStore
        + StaleConceptProposalStore
        + ToolApprovalStore
        + UserStore
        + VerificationStore
        + WorkspaceStore
{
}
