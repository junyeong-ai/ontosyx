use serde::Serialize;
use utoipa::openapi::security::{ApiKey, ApiKeyValue, SecurityScheme};
use utoipa::{Modify, OpenApi, ToSchema};

use crate::routes::{
    acl, ambiguity, approvals, audit, auth, chat, community_summaries, config, dashboards,
    evaluation, federation_admin, governance_audit, governance_routing, health, insights,
    knowledge, lineage, load, models, notifications, ontology, perspectives, pins, prompts_admin,
    quality, query, recipes, reports, schedules, sessions, sources, usage, users,
    verified_queries, workspaces,
};

// Module aliases for utoipa path resolution — utoipa generates hidden __path_*
// structs in the module where #[utoipa::path] is applied, so we must reference
// the actual defining module, not the re-export.
use crate::routes::ontology_drafts::analysis;
use crate::routes::ontology_drafts::decisions;
use crate::routes::ontology_drafts::edit;
use crate::routes::ontology_drafts::extend;
use crate::routes::ontology_drafts::lifecycle;
use crate::routes::ontology_drafts::preview;
use crate::routes::ontology_drafts::refinement;
use crate::routes::ontology_drafts::revisions as ontology_draft_revisions;
use crate::routes::ontology_drafts::scope;
use crate::routes::ontology_drafts::streaming;
use crate::routes::ontology_drafts::types;

// ---------------------------------------------------------------------------
// ErrorResponse — mirrors the JSON body emitted by AppError::into_response()
// ---------------------------------------------------------------------------

#[derive(Serialize, ToSchema)]
pub struct ErrorBody {
    /// Machine-readable error code (e.g., "not_found", "bad_request").
    pub code: String,
    /// Error class: `client_error` for 4xx, `server_error` for 5xx.
    pub class: String,
    /// Interpolation parameters consumed by the frontend i18n catalog.
    pub params: serde_json::Value,
}

#[derive(Serialize, ToSchema)]
pub struct ErrorResponse {
    pub error: ErrorBody,
}

// ---------------------------------------------------------------------------
// Security scheme modifier
// ---------------------------------------------------------------------------

struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "api_key",
                SecurityScheme::ApiKey(ApiKey::Header(ApiKeyValue::new("x-api-key"))),
            );
        }
    }
}

// ---------------------------------------------------------------------------
// OpenAPI document — all paths and schemas registered here
// ---------------------------------------------------------------------------

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Ontosyx API",
        version = env!("CARGO_PKG_VERSION"),
        description = "The Semantic Orchestrator — Knowledge Graph Lifecycle Platform.\n\n\
            ## Response envelope\n\n\
            Every successful 2xx response is wrapped in:\n\n\
            ```json\n\
            { \"data\": <T>, \"pagination\": {\"next_cursor\": \"...\"}?, \"meta\": {...}? }\n\
            ```\n\n\
            Each handler's `responses(... body = T)` documents the type of `data`, \
            **not** the wire shape. Generated clients should either unwrap `data` \
            in a post-processing step (the approach `web/src/lib/api/client.ts` \
            takes — frontend callers receive `T` directly) or wrap every response \
            type in the envelope shape when consuming the spec from a third-party \
            codegen.\n\n\
            Error responses use a separate envelope: `{ \"error\": { \"code\": \"...\", \"class\": \"client_error\", \"params\": {...} } }` \
            (see the `ErrorResponse` schema).",
        license(name = "MIT"),
    ),
    tags(
        (name = "Health", description = "Service health check"),
        (name = "Auth", description = "Platform authentication (OIDC token exchange)"),
        (name = "Users", description = "User directory + role administration"),
        (name = "Chat", description = "Natural language query pipeline"),
        (name = "Query", description = "Raw query execution and history"),
        (name = "Ontology Drafts", description = "Ontology draft lifecycle management"),
        (name = "Ontology", description = "Ontology management and export"),
        (name = "Workspaces", description = "Multi-tenant workspace + membership"),
        (name = "Dashboards", description = "Dashboard + widget composition"),
        (name = "Pins", description = "Pinboard — saved query results"),
        (name = "Perspectives", description = "Workbench canvas perspectives"),
        (name = "Config", description = "System configuration"),
        (name = "Load", description = "Data loading into graph database"),
        (name = "System", description = "System administration"),
        (name = "Admin", description = "Platform administration (admin role required)"),
        (name = "Approvals", description = "Approval queue + comment thread"),
        (name = "Audit", description = "Workspace-wide PROV-O audit trail"),
        (name = "Notifications", description = "Webhook channel routing + delivery log"),
        (name = "Collaboration", description = "Realtime WebSocket protocol (schema-only — see /ws/collab)"),
        (name = "Lineage", description = "Ontology ↔ source binding lineage"),
        (name = "Schedules", description = "Cron-style recipe scheduling"),
        (name = "Reports", description = "Saved report templates + execution"),
        (name = "Sessions", description = "Agent session events + HITL approvals"),
        (name = "ACL", description = "Workspace access-control policies"),
        (name = "Recipes", description = "Reusable analysis recipe templates"),
        (name = "Knowledge", description = "Knowledge base entries + admin review"),
        (name = "Models", description = "LLM model configs + routing rules"),
        (name = "Quality", description = "SHACL-style data-quality rules + adaptive baselines"),
    ),
    paths(
        // Health
        health::health_check,
        health::healthz,
        // Chat
        chat::chat_stream,
        // Ambiguity
        ambiguity::list_ambiguities,
        ambiguity::get_ambiguity,
        ambiguity::resolve_ambiguity,
        ambiguity::revoke_ambiguity,
        ambiguity::bulk_revoke_ambiguities,
        // Query
        query::raw_query,
        query::execute_from_ir,
        query::execute_from_ir_federation,
        query::compile_pattern,
        query::decompile_pattern,
        query::create_saved_pattern,
        query::list_saved_patterns,
        query::get_saved_pattern,
        query::update_saved_pattern,
        query::delete_saved_pattern,
        query::list_executions,
        query::get_execution,
        query::update_feedback,
        query::search_graph,
        query::expand_node,
        query::graph_overview,
        // Evaluation
        evaluation::create_evaluation_run,
        evaluation::get_evaluation_run,
        evaluation::list_evaluation_runs,
        evaluation::complete_evaluation_run,
        evaluation::delete_evaluation_run,
        evaluation::upsert_evaluation_case,
        evaluation::list_evaluation_cases,
        evaluation::upsert_evaluation_metric,
        evaluation::bulk_upsert_evaluation_cases,
        evaluation::execute_evaluation_case,
        evaluation::judge_evaluation_case,
        evaluation::judge_safety_evaluation_case,
        evaluation::list_evaluation_metrics,
        evaluation::upsert_evaluation_dataset,
        evaluation::promote_case_to_dataset,
        evaluation::list_evaluation_datasets,
        evaluation::get_evaluation_dataset,
        evaluation::delete_evaluation_dataset,
        evaluation::list_evaluation_dataset_items,
        evaluation::replace_evaluation_dataset_items,
        evaluation::create_run_from_dataset,
        evaluation::evaluation_run_summary,
        evaluation::list_run_comparison_outliers,
        evaluation::compare_evaluation_runs,
        // Ontology drafts — lifecycle
        lifecycle::create_ontology_draft,
        lifecycle::list_ontology_drafts,
        lifecycle::get_ontology_draft,
        lifecycle::delete_ontology_draft,
        lifecycle::complete_ontology_draft,
        lifecycle::deploy_schema,
        lifecycle::generate_load_plan,
        lifecycle::compile_load,
        lifecycle::execute_load_from_source,
        decisions::update_decisions,
        refinement::design_ontology_draft,
        refinement::refine_ontology_draft,
        refinement::apply_reconcile,
        analysis::reanalyze_ontology_draft,
        analysis::reanalyze_modeled_ontology_draft,
        scope::include_scope_tables,
        scope::defer_scope_tables,
        edit::edit_ontology_draft,
        extend::extend_ontology_draft,
        preview::preview_source,
        streaming::design_ontology_draft_stream,
        streaming::refine_ontology_draft_stream,
        // Ontology drafts — revisions
        ontology_draft_revisions::list_revisions,
        ontology_draft_revisions::get_revision,
        ontology_draft_revisions::restore_revision,
        ontology_draft_revisions::diff_revisions,
        ontology_draft_revisions::diff_current,
        ontology_draft_revisions::diff_canonical,
        ontology_draft_revisions::rebase_preview,
        ontology_draft_revisions::rebase_draft,
        ontology_draft_revisions::migrate_schema,
        // Ontology
        ontology::create_ontology,
        ontology::get_workspace_ontology_detail,
        ontology::list_canonical_versions,
        ontology::apply_ontology_edits,
        ontology::map_summary,
        ontology::list_axis_items,
        ontology::list_cross_refs,
        ontology::normalize_ontology,
        ontology::export_ontology,
        ontology::apply_ontology_commands,
        ontology::export_cypher,
        ontology::export_mermaid,
        ontology::export_graphql,
        ontology::export_owl,
        ontology::export_shacl,
        ontology::export_typescript,
        ontology::export_python,
        ontology::import_owl,
        ontology::propose_ontology_value_sets,
        ontology::propose_ontology_notation_patterns,
        ontology::suggest_insights,
        ontology::reindex_schema,
        ontology::graph_audit_report,
        ontology::adopt_graph,
        ontology::enrich_ontology,
        ontology::suggest_concept_property_bindings,
        ontology::suggest_concepts_for_property,
        community_summaries::list_community_summaries,
        community_summaries::upsert_community_summary,
        community_summaries::search_community_summaries,
        community_summaries::delete_community_summary,
        ontology::get_ontology_dependencies,
        ontology::get_ontology_validate,
        // Pins
        pins::create_pin,
        pins::list_pins,
        pins::delete_pin,
        // Persisted insights
        insights::create_insight,
        insights::update_insight,
        insights::get_insight,
        insights::list_insights,
        insights::delete_insight,
        // Perspectives
        perspectives::save_perspective,
        perspectives::list_perspectives,
        perspectives::find_default_perspective,
        perspectives::find_best_perspective,
        perspectives::delete_perspective,
        // Config
        config::get_config,
        config::get_ui_config,
        config::update_config,
        // Load
        load::plan_load,
        load::execute_load,
        load::list_load_checkpoints,
        load::delete_load_checkpoint,
        load::list_prompts,
        // Sources
        sources::test_source_connection,
        // Admin — prompt template CRUD
        prompts_admin::list_prompt_templates,
        prompts_admin::get_prompt_template,
        prompts_admin::create_prompt_template,
        prompts_admin::update_prompt_template,
        prompts_admin::delete_prompt_template,
        // Admin — federation adapter registry
        federation_admin::list_adapters,
        federation_admin::register_adapter,
        federation_admin::get_adapter,
        federation_admin::preview_adapter,
        federation_admin::refresh_adapters,
        federation_admin::delete_adapter,
        federation_admin::federation_health,
        federation_admin::list_adapter_tables,
        federation_admin::analyze_adapter,
        federation_admin::get_adapter_analysis,
        // Approvals
        approvals::list_approvals,
        approvals::get_approval,
        approvals::review_approval,
        approvals::bulk_review_approvals,
        approvals::list_approval_comments,
        approvals::create_approval_comment,
        // Audit
        governance_audit::list_audit_records,
        // Admin — governance routing
        governance_routing::list_routing_rules,
        governance_routing::upsert_routing_rule,
        governance_routing::delete_routing_rule,
        // Workspace context (locale chain + role for the FE)
        workspaces::workspace_me,
        // Workspaces — CRUD + membership
        workspaces::create_workspace,
        workspaces::list_workspaces,
        workspaces::get_workspace,
        workspaces::update_workspace,
        workspaces::update_workspace_locale,
        workspaces::delete_workspace,
        workspaces::add_member,
        workspaces::remove_member,
        workspaces::update_member_role,
        workspaces::list_members,
        // Auth
        auth::create_token,
        auth::me,
        auth::ws_token,
        auth::logout,
        // Users
        users::list_users,
        users::update_user_role,
        // Dashboards + widgets
        dashboards::create_dashboard,
        dashboards::list_dashboards,
        dashboards::get_dashboard,
        dashboards::update_dashboard,
        dashboards::delete_dashboard,
        dashboards::add_widget,
        dashboards::list_widgets,
        dashboards::update_widget,
        dashboards::delete_widget,
        dashboards::share_dashboard,
        dashboards::unshare_dashboard,
        dashboards::get_shared_dashboard,
        // Notifications
        notifications::create_channel,
        notifications::list_channels,
        notifications::update_channel,
        notifications::delete_channel,
        notifications::test_channel,
        notifications::list_logs,
        // Audit + usage
        audit::list_audit_events,
        usage::get_usage_summary,
        // Lineage
        lineage::get_lineage_summary,
        lineage::list_lineage_for_label,
        lineage::get_lineage_for_project,
        // Schedules
        schedules::create_schedule,
        schedules::list_schedules,
        schedules::get_schedule,
        schedules::update_schedule,
        schedules::delete_schedule,
        // Reports
        reports::create_report,
        reports::list_reports,
        reports::get_report,
        reports::update_report,
        reports::delete_report,
        reports::execute_report,
        // Sessions
        sessions::list_sessions,
        sessions::get_session,
        sessions::list_session_events,
        sessions::get_session_messages,
        sessions::delete_session,
        sessions::respond_tool_review,
        // ACL
        acl::create_policy,
        acl::list_policies,
        acl::get_policy,
        acl::update_policy,
        acl::delete_policy,
        acl::effective_policies,
        // Recipes
        recipes::create_recipe,
        recipes::list_recipes,
        recipes::get_recipe,
        recipes::delete_recipe,
        recipes::list_recipe_results,
        recipes::update_recipe_status,
        recipes::create_recipe_version,
        recipes::list_recipe_versions,
        // Knowledge
        knowledge::create_knowledge,
        knowledge::list_knowledge,
        knowledge::get_knowledge,
        knowledge::update_knowledge,
        knowledge::delete_knowledge,
        knowledge::update_status,
        knowledge::list_stale,
        knowledge::knowledge_stats,
        knowledge::bulk_review,
        // Verified query bank (Φ11)
        verified_queries::promote_verified_query,
        verified_queries::list_verified_queries,
        verified_queries::get_verified_query,
        verified_queries::transition_verified_query_status,
        verified_queries::delete_verified_query,
        // Models
        models::list_model_operations,
        models::list_model_configs,
        models::create_model_config,
        models::update_model_config,
        models::delete_model_config,
        models::list_routing_rules,
        models::create_routing_rule,
        models::update_routing_rule,
        models::delete_routing_rule,
        models::test_model_connection,
        // Ontology — type-candidates / verifications
        ontology::list_type_candidates,
        ontology::verify_element,
        ontology::list_verifications,
        ontology::delete_verification,
        // Quality
        quality::create_rule,
        quality::list_rules,
        quality::get_rule,
        quality::update_rule,
        quality::delete_rule,
        quality::quality_dashboard,
        quality::rule_results,
        quality::execute_rule,
        quality::execute_all_rules,
        quality::get_quality_metrics,
        quality::list_shacl_failures,
        quality::get_quality_baseline,
        quality::list_stale_types,
        quality::list_stale_proposals,
        quality::decide_stale_proposal,
        quality::bulk_decide_stale_proposals,
    ),
    components(
        schemas(
            ErrorResponse,
            ErrorBody,
            // Universal envelope companion type. `ApiResponse` itself is
            // generic over the per-handler payload `T`; we publish the
            // pagination side-car here so list responses can reference it
            // by name from the path docs.
            crate::response::PageMeta,
            // Health probe wire shape — shared by /api/health (wrapped)
            // and /api/healthz (flat).
            health::HealthResponse,
            health::HealthComponents,
            health::HealthLlm,
            // Chat
            chat::ChatStreamRequest,
            // Ambiguity
            ambiguity::AmbiguitySummary,
            ambiguity::AmbiguityListResponse,
            ambiguity::AmbiguityDetailResponse,
            ambiguity::ResolveAmbiguityRequest,
            ambiguity::RevokeAmbiguityResponse,
            ambiguity::BulkRevokeAmbiguitiesRequest,
            ambiguity::BulkRevokeAmbiguitiesResponse,
            ox_ontology::ambiguity::AmbiguityContext,
            ox_ontology::ambiguity::AmbiguityKind,
            ox_ontology::ambiguity::RepoHint,
            ox_ontology::ambiguity::AmbiguityResolution,
            ox_ontology::ambiguity::AmbiguityMapping,
            ox_ontology::ambiguity::ValueMapEntry,
            // Query
            query::ExecuteRawQueryRequest,
            query::ExecuteRawQueryResponse,
            query::ExecuteFromIrRequest,
            query::ExecuteFromIrResponse,
            query::CompilePatternRequest,
            query::CompilePatternResponse,
            query::DecompilePatternRequest,
            query::DecompilePatternResponse,
            query::CreateSavedPatternRequest,
            query::UpdateSavedPatternRequest,
            query::SavedPatternResponse,
            query::SearchGraphRequest,
            query::ExpandGraphRequest,
            ox_query_ir::query::QueryIR,
            ox_query_ir::query::QueryOp,
            ox_query_ir::query::GraphFunction,
            ox_query_ir::query::GraphAlgorithm,
            ox_query_ir::query::AnalyticsSource,
            ox_query_ir::query::GraphPattern,
            ox_query_ir::query::PropertyFilter,
            ox_query_ir::query::VarLength,
            ox_query_ir::query::PathElement,
            ox_query_ir::query::Expr,
            ox_query_ir::query::WhenClause,
            ox_query_ir::query::ComparisonOp,
            ox_query_ir::query::LogicalOp,
            ox_query_ir::query::StringOp,
            ox_query_ir::query::Projection,
            ox_query_ir::query::AggregationExpr,
            ox_query_ir::query::AggFunction,
            ox_query_ir::query::FieldRef,
            ox_query_ir::query::NodeRef,
            ox_query_ir::query::OrderClause,
            ox_query_ir::query::SortDirection,
            ox_query_ir::query::PathAlgorithm,
            ox_query_ir::query::ChainStep,
            ox_query_ir::query::MutateOp,
            ox_query_ir::query::PropertyAssignment,
            ox_query_ir::pattern::PatternIR,
            ox_query_ir::pattern::ReadOnlyReason,
            ox_query_ir::pattern::PatternNode,
            ox_query_ir::pattern::PatternEdge,
            ox_query_ir::pattern::PatternFilter,
            ox_query_ir::pattern::PatternProjection,
            ox_query_ir::pattern::LayoutHints,
            ox_query_ir::pattern::Position,
            ox_query_ir::hybrid_retrieval::Embedding,
            ox_query_ir::hybrid_retrieval::FusionStrategy,
            ox_query_ir::hybrid_retrieval::HybridSearchRequest,
            ox_query_ir::widget::WidgetHint,
            ox_query_ir::widget::WidgetType,
            ox_ontology::graph_exploration::SearchResultNode,
            ox_ontology::graph_exploration::ExpandNeighbor,
            ox_ontology::graph_exploration::NodeExpansion,
            ox_ontology::graph_exploration::LabelStat,
            ox_ontology::graph_exploration::RelationshipPattern,
            ox_ontology::graph_exploration::PropertySchema,
            ox_ontology::graph_exploration::GraphSchemaOverview,
            // Evaluation
            evaluation::CreateEvaluationRunRequest,
            evaluation::CompleteEvaluationRunRequest,
            evaluation::UpsertEvaluationCaseRequest,
            evaluation::RecordEvaluationMetricRequest,
            evaluation::EvaluationRunResponse,
            evaluation::EvaluationCaseResponse,
            evaluation::EvaluationMetricResponse,
            ox_store::evaluation::EvaluationRunStatus,
            ox_store::evaluation::EvaluationRun,
            ox_store::evaluation::EvaluationCase,
            ox_store::evaluation::EvaluationCaseInput,
            ox_store::evaluation::EvaluationExpected,
            ox_store::evaluation::EvaluationActual,
            ox_store::evaluation::RetrievalSurface,
            ox_store::evaluation::RetrievalLeg,
            ox_store::evaluation::RetrievalComparisonAggregate,
            ox_store::evaluation::RetrievalComparisonDelta,
            ox_store::evaluation::RetrievalComparisonOutlier,
            ox_store::evaluation::RetrievalLiftRegressionAlert,
            ox_store::evaluation::EvaluationRetrievedAnchor,
            ox_store::evaluation::EvaluationCaseMetadata,
            ox_store::evaluation::EvaluationMetricMetadata,
            ox_store::evaluation::EvaluationJudgeSource,
            ox_store::evaluation::EvaluationCaptureAxis,
            ox_ontology::EvaluationFingerprint,
            ox_ontology::EvaluationFingerprintInput,
            ox_ontology::ModelCall,
            ox_ontology::ModelPrices,
            ox_ontology::ConfigHash,
            ox_ontology::ModelId,
            ox_ontology::PromptTemplateId,
            ox_ontology::RetrievalProfileId,
            ox_ontology::PipelineStage,
            ox_ontology::StageOutcome,
            ox_ontology::ErrorClassification,
            ox_ontology::SessionOutcome,
            ox_ontology::AttemptOutcome,
            ox_ontology::InferenceSession,
            ox_ontology::InferenceAttempt,
            ox_ontology::RetrievalProfile,
            ox_ontology::TraversalStrategy,
            ox_ontology::RetrievalLimits,
            ox_ontology::CommunityDetectionPolicy,
            ox_ontology::CommunityDetectionPolicyId,
            ox_ontology::VerifiedQueryDef,
            ox_ontology::VerifiedQueryId,
            ox_ontology::ComplexityClass,
            ox_ontology::VerifiedQueryStatus,
            verified_queries::PromoteVerifiedQueryRequest,
            verified_queries::TransitionVerifiedQueryStatusRequest,
            verified_queries::VerifiedQueryListResponse,
            ox_store::evaluation::EvaluationMetric,
            ox_store::evaluation::EvaluationDataset,
            ox_store::evaluation::EvaluationDatasetItem,
            ox_store::evaluation::EvaluationDatasetSummary,
            ox_store::evaluation::AxisAggregate,
            ox_store::evaluation::RunSummary,
            ox_store::evaluation::RunMetricDelta,
            ox_store::evaluation::RunAxisSummary,
            ox_store::evaluation::RunComparisonReport,
            evaluation::BulkUpsertEvaluationCasesRequest,
            evaluation::BulkUpsertEvaluationCasesResponse,
            evaluation::ExecuteEvaluationCaseResponse,
            evaluation::JudgeEvaluationCaseResponse,
            evaluation::UpsertEvaluationDatasetRequest,
            evaluation::ReplaceEvaluationDatasetItemsRequest,
            evaluation::EvaluationDatasetResponse,
            evaluation::ReplaceEvaluationDatasetItemsResponse,
            evaluation::CreateRunFromDatasetRequest,
            evaluation::CreateRunFromDatasetResponse,
            evaluation::PromoteCaseToDatasetRequest,
            evaluation::PromoteCaseToDatasetResponse,
            // Ontology drafts
            types::CreateOntologyDraftRequest,
            types::OntologyDraftOrigin,
            types::DataSourceSpec,
            types::OntologyDraftView,
            types::AnalysisReportStatus,
            ox_ontology::ontology_draft::SourceConfig,
            ox_ontology::ontology_draft::SourceHistoryEntry,
            ox_ontology::ontology_draft::SourceTypeKind,
            ox_core::source_schema::SourceSchema,
            ox_core::source_schema::SourceTableDef,
            ox_core::source_schema::SourceColumnDef,
            ox_core::source_schema::ForeignKeyDef,
            ox_core::source_schema::SourceProfile,
            ox_core::source_schema::TableProfile,
            ox_core::source_schema::ColumnStats,
            ox_core::source_schema::PiiSuspectKind,
            ox_core::source_scope::AnalysisScope,
            ox_core::source_scope::DeferredTable,
            ox_ontology::source_analysis::SourceAnalysisReport,
            ox_ontology::source_analysis::SchemaStats,
            ox_ontology::source_analysis::AnalysisCompleteness,
            ox_ontology::source_analysis::AnalysisWarning,
            ox_ontology::source_analysis::WarningLevel,
            ox_ontology::source_analysis::AnalysisPhase,
            ox_ontology::source_analysis::WarningClass,
            ox_ontology::source_analysis::WarningScope,
            ox_ontology::source_analysis::ImpliedRelationship,
            ox_ontology::source_analysis::ImpliedFkPattern,
            ox_ontology::source_analysis::TableExclusionSuggestion,
            ox_ontology::source_analysis::TableExclusionReason,
            ox_ontology::source_analysis::LargeSchemaWarning,
            ox_ontology::source_analysis::RepoColumnSuggestion,
            ox_ontology::source_analysis::RepoAnalysisStatus,
            ox_ontology::source_analysis::RepoFailureKind,
            ox_ontology::source_analysis::RepoAnalysisSummary,
            ox_ontology::repo_insights::RepoSource,
            ox_ontology::source_analysis::DesignOptions,
            ox_ontology::source_analysis::ConfirmedRelationship,
            ox_ontology::source_analysis::ColumnClarification,
            types::UpdateOntologyDraftDecisionsRequest,
            types::DesignOntologyDraftRequest,
            types::DesignOntologyDraftResponse,
            types::ReanalyzeOntologyDraftRequest,
            types::ReanalyzeOntologyDraftResponse,
            analysis::ReanalyzeModeledOntologyDraftRequest,
            types::RefineOntologyDraftRequest,
            types::RefineOntologyDraftResponse,
            types::ReconcileOntologyDraftRequest,
            ox_ontology::ReconcileReport,
            ox_ontology::PreservedEntity,
            ox_ontology::GeneratedEntity,
            ox_ontology::UncertainMatch,
            ox_ontology::DeletedEntity,
            ox_ontology::ReconcileEntityKind,
            ox_ontology::ReconcileConfidence,
            ox_ontology::MatchDecision,
            types::ExtendOntologyDraftRequest,
            types::ExtendOntologyDraftResponse,
            types::CompleteOntologyDraftRequest,
            ox_ontology::quality::OntologyQualityReport,
            ox_ontology::quality::QualityConfidence,
            ox_ontology::quality::QualityGap,
            ox_ontology::quality::QualityGapRef,
            ox_ontology::quality::QualityGapSeverity,
            ox_ontology::quality::QualityGapCategory,
            lifecycle::DeployOntologyDraftSchemaRequest,
            lifecycle::DeployOntologyDraftSchemaResponse,
            lifecycle::GenerateOntologyDraftLoadPlanResponse,
            lifecycle::CompileOntologyDraftLoadPlanRequest,
            lifecycle::CompileOntologyDraftLoadPlanResponse,
            lifecycle::ExecuteOntologyDraftLoadRequest,
            lifecycle::ExecuteOntologyDraftLoadResponse,
            lifecycle::LoadResultResponse,
            lifecycle::LoadErrorResponse,
            ox_ontology::load_plan::LoadPlan,
            ontology_draft_revisions::RestoreOntologyDraftRevisionResponse,
            ox_ontology::OntologyDiff,
            ox_ontology::NodeDiff,
            ox_ontology::NodeChange,
            ox_ontology::PropertyChange,
            ox_ontology::EdgeDiff,
            ox_ontology::EdgeChange,
            ox_ontology::DiffSummary,
            ox_ontology::rebase::RebaseAnalysis,
            ox_ontology::rebase::RebaseConflict,
            ox_ontology::rebase::ConflictEntityKind,
            ox_ontology::rebase::ConflictSide,
            ox_ontology::rebase::ConflictAxis,
            ox_ontology::rebase::PropertyConflictAxis,
            ontology_draft_revisions::RebasePreviewResponse,
            ontology_draft_revisions::RebaseOntologyDraftRequest,
            ontology_draft_revisions::RebaseOntologyDraftResponse,
            ontology_draft_revisions::MigrateOntologyDraftSchemaRequest,
            ontology_draft_revisions::MigrateOntologyDraftSchemaResponse,
            types::EditOntologyDraftRequest,
            types::EditOntologyDraftResponse,
            preview::PreviewSourceRequest,
            preview::PreviewSourceResponse,
            preview::PreviewTableSummary,
            scope::IncludeScopeTablesRequest,
            scope::DeferScopeTablesRequest,
            scope::ScopeUpdateResponse,
            // Sources
            sources::TestConnectionRequest,
            sources::TestConnectionResponse,
            // Ontology
            ontology::CreateOntologyRequest,
            ontology::CreateOntologyResponse,
            ontology::ApplyOntologyCommandsRequest,
            ontology::ApplyOntologyCommandsResponse,
            ox_ontology::EditOntologyRequest,
            ox_ontology::OntologyEditReceipt,
            ox_ontology::OntologyEditPreCheck,
            ontology::EditOntologyResponse,
            ox_store::ElementVerification,
            ontology::NormalizeOntologyResponse,
            ontology::ImportOntologyRequest,
            ox_ontology::input::InputOntologyDef,
            ox_ontology::input::InputNodeTypeDef,
            ox_ontology::input::InputEdgeTypeDef,
            ox_ontology::input::InputPropertyDef,
            ox_ontology::input::InputNodeConstraint,
            ox_ontology::input::InputIndexDef,
            ox_ontology::input::NormalizeWarning,
            ontology::AdoptGraphRequest,
            ox_ontology::audit::GraphAuditReport,
            ox_ontology::audit::SyncStatus,
            ox_ontology::InsightHint,
            ontology::MapSummaryResponse,
            ontology::AxisCounts,
            ontology::AxisEntry,
            ontology::DanglerEntry,
            ontology::AxisItem,
            ontology::AxisItemsParams,
            ontology::Axis,
            ontology::CrossRefEdge,
            ontology::ProposeValueSetsRequest,
            ontology::ProposeValueSetsResponse,
            ontology::ProposePolicyBody,
            ontology::ProposalBody,
            ontology::EvidenceBody,
            ontology::SkipBody,
            ontology::ProposeNotationPatternsRequest,
            ontology::ProposeNotationPatternsResponse,
            ontology::NotationPolicyBody,
            ontology::NotationProposalBody,
            ontology::NotationSkipBody,
            ontology::BindingPolicy,
            ontology::SuggestBindingsRequest,
            ontology::SuggestBindingsResponse,
            ontology::SuggestConceptsRequest,
            ontology::SuggestConceptsResponse,
            ontology::PropertyCandidate,
            ontology::ConceptCandidate,
            community_summaries::UpsertCommunitySummaryRequest,
            community_summaries::CommunitySummaryDto,
            community_summaries::CommunitySummaryResponse,
            community_summaries::ListCommunitySummariesResponse,
            community_summaries::SearchCommunitySummariesQuery,
            ontology::TypeCandidate,
            ontology::VerifyElementRequest,
            ontology::VerifyElementResponse,
            ox_ontology::binding_suggestions::BindingSignal,
            ox_ontology::SchemaDependencyGraph,
            ox_ontology::DependencyBucket,
            ox_ontology::DependencyEdge,
            ox_ontology::DependencyKind,
            ox_ontology::SchemaEntityRef,
            federation_admin::AdapterTableSummary,
            federation_admin::AdapterTableListResponse,
            federation_admin::AnalyzeAdapterRequest,
            federation_admin::AnalyzeAdapterResponse,
            federation_admin::AdapterAnalysisDriftEntry,
            federation_admin::AdapterAnalysisResponse,
            ox_source::AnalysisResult,
            ox_source::AnalyzeSelection,
            // Pins
            pins::CreatePinRequest,
            // Perspectives
            perspectives::UpsertPerspectiveRequest,
            perspectives::PerspectiveFindParams,
            perspectives::DeletePerspectiveResponse,
            // Config
            config::ConfigEntry,
            config::ConfigResponse,
            config::UiConfig,
            config::UpdateConfigRequest,
            config::ConfigUpdate,
            config::ConfigUpdateResponse,
            // Load
            load::GenerateLoadPlanRequest,
            load::GenerateLoadPlanResponse,
            load::ExecuteLoadRequest,
            load::ExecuteLoadResponse,
            load::LoadExecutionResult,
            load::LoadExecutionError,
            load::PromptInfo,
            // Revisions
            ontology_draft_revisions::RestoreOntologyDraftRevisionResponse,
            // Admin — prompt templates
            prompts_admin::CreatePromptRequest,
            prompts_admin::UpdatePromptRequest,
            federation_admin::RegisterAdapterRequest,
            federation_admin::RegisterAdapterKind,
            federation_admin::RegisterAdapterResponse,
            federation_admin::AdapterSummary,
            federation_admin::AdapterDetail,
            federation_admin::AdapterDetailKind,
            federation_admin::CredentialSource,
            federation_admin::RefreshAdaptersResponse,
            federation_admin::PreviewAdapterRequest,
            federation_admin::PreviewAdapterResponse,
            federation_admin::PreviewTable,
            federation_admin::PreviewColumn,
            federation_admin::FederationHealthResponse,
            crate::credential::Credential,
            PromptTemplateRow,
            // Store models
            CursorParams,
            ox_store::OntologyDraft,
            ox_store::OntologyDraftSummary,
            ontology::OntologyDetail,
            ontology::CurrentVersionSummary,
            ontology::WorkspaceOntologyResponse,
            ontology::OntologyVersionEntry,
            ontology::OntologyVersionsResponse,
            workspaces::WorkspaceMeResponse,
            workspaces::WorkspaceResponse,
            workspaces::WorkspaceSummaryResponse,
            workspaces::MemberResponse,
            workspaces::DeleteWorkspaceResponse,
            workspaces::RemoveMemberResponse,
            workspaces::CreateWorkspaceRequest,
            workspaces::UpdateWorkspaceRequest,
            workspaces::UpdateWorkspaceLocaleRequest,
            workspaces::AddMemberRequest,
            workspaces::UpdateMemberRoleRequest,
            // Auth
            auth::CreateAuthTokenRequest,
            auth::CreateAuthTokenResponse,
            auth::AuthMeResponse,
            auth::LogoutResponse,
            auth::UserInfo,
            auth::WebSocketTokenResponse,
            // Users
            users::UpdateUserRoleRequest,
            users::UserRoleUpdateResponse,
            // Dashboards
            ox_store::Dashboard,
            ox_store::DashboardLayoutItem,
            ox_store::DashboardWidget,
            ox_store::DashboardWidgetPosition,
            ox_store::DashboardWidgetThresholds,
            ox_store::ThresholdDirection,
            dashboards::CreateDashboardRequest,
            dashboards::UpdateDashboardRequest,
            dashboards::CreateWidgetRequest,
            dashboards::WidgetUpdateRequest,
            dashboards::ShareDashboardRequest,
            dashboards::ShareDashboardResponse,
            dashboards::SharedDashboardResponse,
            dashboards::SharedWidgetResponse,
            ox_store::QueryExecution,
            ox_store::QueryExecutionSummary,
            ox_store::PinboardItem,
            ox_store::WorkbenchPerspective,
            ox_store::CanvasPosition,
            ox_store::CanvasViewport,
            ox_store::LoadCheckpoint,
            ox_store::OntologySnapshot,
            ox_store::OntologySnapshotSummary,
            // Approvals
            ox_store::ApprovalRequest,
            ox_store::ApprovalComment,
            approvals::ReviewApprovalRequest,
            approvals::ReviewApprovalResponse,
            approvals::BulkReviewApprovalsRequest,
            approvals::BulkReviewApprovalsResponse,
            approvals::CreateApprovalCommentRequest,
            // Audit
            ox_store::AuditEntry,
            ox_store::AuditRecord,
            AuditRecordPage,
            AuditEntryPage,
            UserInfoPage,
            DashboardPage,
            AgentSessionPage,
            AnalysisRecipePage,
            SavedReportPage,
            QueryExecutionSummaryPage,
            PinboardItemPage,
            SavedPatternResponsePage,
            KnowledgeEntryPage,
            OntologyDraftSummaryPage,
            EvaluationRunPage,
            EvaluationDatasetSummaryPage,
            ox_ontology::provenance::ProvenanceCapture,
            ox_ontology::provenance::ProvenanceDef,
            ox_ontology::provenance::ProvenancePlan,
            ox_ontology::provenance::ProvenanceId,
            ox_ontology::provenance::EntityRef,
            ox_ontology::provenance::ProvenanceActivityKind,
            ox_ontology::provenance::ValidationOutcomeKind,
            ox_ontology::provenance::AgentRef,
            // Governance routing — wire types come from ox-ontology so the
            // route layer never reshapes the canonical IR enums.
            governance_routing::ChangeRoutingRuleResponse,
            governance_routing::UpsertRoutingRuleRequest,
            ox_ontology::change_routing::ChangeType,
            ox_ontology::change_routing::ApprovalRouting,
            ox_ontology::change_routing::ApprovalSkipPredicate,
            ox_ontology::change_routing::RoleRef,
            ox_ontology::change_routing::ScopeKind,
            ox_ontology::change_routing::RiskLevel,
            // Collaboration — WebSocket wire protocol (no HTTP path).
            // Both halves of the protocol are emitted as schemas so
            // generated clients see the same `discriminated union`
            // shape the server reads / writes.
            crate::collaboration::ClientMessage,
            crate::collaboration::ServerMessage,
            crate::collaboration::ErrorCode,
            crate::collaboration::PresenceInfo,
            crate::collaboration::CursorPosition,
            crate::collaboration::LockSnapshot,
            // Notifications
            notifications::CreateChannelRequest,
            notifications::UpdateChannelRequest,
            notifications::TestChannelResponse,
            ox_store::NotificationChannelType,
            ox_store::WebhookNotificationConfig,
            ox_store::NotificationChannel,
            ox_store::NotificationLog,
            // Schedules
            schedules::CreateScheduleRequest,
            schedules::UpdateScheduleRequest,
            ox_store::ScheduledTask,
            // Reports
            reports::CreateReportRequest,
            reports::UpdateReportRequest,
            reports::ExecuteReportRequest,
            ox_store::SavedReport,
            ox_store::SavedReportParameter,
            ox_query_ir::query::QueryResult,
            ox_query_ir::query::QueryMetadata,
            ox_query_ir::query::QueryDiagnostic,
            ox_query_ir::query::DiagnosticLevel,
            ox_query_ir::query::QueryProvenance,
            ox_query_ir::query::ColumnLineage,
            // Sessions
            ox_store::AgentSession,
            ox_store::AgentSessionModelConfig,
            ox_store::AgentExecutionMode,
            ox_store::AgentEvent,
            ox_store::AgentEventPayload,
            sessions::SessionMessagesResponse,
            sessions::SessionChatMessage,
            sessions::SessionToolCall,
            sessions::SessionUsage,
            sessions::ToolRespondRequest,
            sessions::ToolRespondResponse,
            insights::CreateInsightRequest,
            insights::UpdateInsightRequest,
            insights::InsightResponse,
            ox_query_ir::insight::InsightDef,
            ox_store::CursorPage<ox_query_ir::insight::InsightDef>,
            // ACL
            acl::CreatePolicyRequest,
            acl::UpdatePolicyRequest,
            ox_store::AclPolicy,
            // Recipes
            recipes::CreateRecipeRequest,
            recipes::RecipeStatusUpdateRequest,
            ox_store::AnalysisRecipe,
            ox_store::RecipeStatus,
            ox_store::RecipeExecutionResult,
            // Knowledge
            knowledge::CreateKnowledgeEntryRequest,
            knowledge::UpdateKnowledgeEntryRequest,
            knowledge::UpdateKnowledgeStatusRequest,
            knowledge::BulkReviewApprovalsRequest,
            knowledge::KnowledgeBulkReviewResponse,
            knowledge::KnowledgeStats,
            ox_store::KnowledgeEntry,
            ox_store::KnowledgeKind,
            ox_store::KnowledgeStatus,
            // Models
            ox_store::ModelConfig,
            ox_store::NewModelConfig,
            ox_store::ModelConfigUpdate,
            ox_store::ModelRoutingRule,
            ox_store::NewRoutingRule,
            ox_store::RoutingRuleUpdate,
            models::TestModelRequest,
            models::TestModelResponse,
            models::ModelOperation,
            // Quality
            quality::CreateRuleRequest,
            quality::UpdateRuleRequest,
            ox_store::QualityRule,
            ox_store::QualityResult,
            ox_store::QualityDashboardEntry,
            ox_store::QualityMetricsReport,
            ox_store::MetricValue,
            ox_store::MetricWindow,
            ox_store::ShaclFailureKind,
            ox_store::ShaclFailureCount,
            ox_store::StaleTypeEntry,
            ox_store::StaleProposalDecision,
            ox_store::StaleConceptProposal,
            ox_store::WorkspaceQualityBaseline,
            quality::DecideStaleProposalRequest,
            quality::BulkDecideStaleProposalsRequest,
            quality::BulkDecideStaleProposalsResponse,
            // Usage / lineage
            ox_store::UsageSummary,
            ox_store::LineageEntry,
            ox_store::LineageSummary,
        ),
    ),
    modifiers(&SecurityAddon),
)]
pub struct ApiDoc;

// ---------------------------------------------------------------------------
// Schema wrappers for ox-store models
//
// These wrap the sqlx-derived models from ox-store with ToSchema so they
// can appear in the OpenAPI spec without adding utoipa as a dep to ox-store.
// ---------------------------------------------------------------------------

/// Cursor-based pagination parameters.
#[derive(ToSchema)]
#[schema(as = CursorParams)]
#[allow(dead_code)]
pub struct CursorParams {
    /// Max items to return (default 50, max 100)
    pub limit: Option<u32>,
    /// Opaque cursor from a previous response's `next_cursor`
    pub cursor: Option<String>,
}

/// Design project — ontology design lifecycle.
/// Design project summary (lightweight, for list endpoints).
/// Saved dashboard — workspace-scoped, owner-private until shared.
/// One widget on a dashboard.
/// Query execution record.
/// Query execution summary (lightweight, for list endpoints).
/// Pinboard item — a saved query result.
/// Workbench perspective — saved canvas state.
/// Ontology revision snapshot.
/// Ontology revision snapshot summary (lightweight).
/// Prompt template row (admin-only — wired by `routes/prompts_admin.rs`).
///
/// `version` is the semver string (e.g. `1.2.3`). `workspace_id = null`
/// means the template is global; a concrete uuid scopes it to one
/// workspace (used by the `/api/admin/prompts` workspace-override flow).
#[derive(ToSchema)]
#[schema(as = PromptTemplateRow)]
#[allow(dead_code)]
pub struct PromptTemplateRow {
    pub id: uuid::Uuid,
    pub name: String,
    pub version: String,
    pub content: String,
    pub variables: Vec<String>,
    #[schema(value_type = std::collections::HashMap<String, Object>, additional_properties)]
    pub metadata: serde_json::Value,
    pub created_by: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub is_active: bool,
    pub workspace_id: Option<uuid::Uuid>,
}

/// Approval request — a queued gated operation awaiting review.
/// One entry in the comment thread attached to an approval. The
/// reviewer's decision-time rationale is the first comment; any
/// pre-/post-decision discussion follows in the same thread.
/// One record in the workspace-wide PROV-O audit stream. The
/// `provenance` payload is the content-addressed `ProvenanceDef`
/// emitted at IR commit time; the surrounding fields attribute it
/// to the source ontology.
/// Cursor-paginated audit page. The wire shape is
/// `{ items: [...], next_cursor?: string }`. `next_cursor` is
/// absent when no further pages exist.
#[derive(ToSchema)]
#[allow(dead_code)]
pub struct AuditRecordPage {
    pub items: Vec<ox_store::AuditRecord>,
    pub next_cursor: Option<String>,
}

#[derive(ToSchema)]
#[allow(dead_code)]
pub struct AuditEntryPage {
    pub items: Vec<ox_store::AuditEntry>,
    pub next_cursor: Option<String>,
}

#[derive(ToSchema)]
#[allow(dead_code)]
pub struct UserInfoPage {
    pub items: Vec<auth::UserInfo>,
    pub next_cursor: Option<String>,
}

#[derive(ToSchema)]
#[allow(dead_code)]
pub struct DashboardPage {
    pub items: Vec<ox_store::Dashboard>,
    pub next_cursor: Option<String>,
}

#[derive(ToSchema)]
#[allow(dead_code)]
pub struct AgentSessionPage {
    pub items: Vec<ox_store::AgentSession>,
    pub next_cursor: Option<String>,
}

#[derive(ToSchema)]
#[allow(dead_code)]
pub struct AnalysisRecipePage {
    pub items: Vec<ox_store::AnalysisRecipe>,
    pub next_cursor: Option<String>,
}

#[derive(ToSchema)]
#[allow(dead_code)]
pub struct SavedReportPage {
    pub items: Vec<ox_store::SavedReport>,
    pub next_cursor: Option<String>,
}

#[derive(ToSchema)]
#[allow(dead_code)]
pub struct QueryExecutionSummaryPage {
    pub items: Vec<ox_store::QueryExecutionSummary>,
    pub next_cursor: Option<String>,
}

#[derive(ToSchema)]
#[allow(dead_code)]
pub struct PinboardItemPage {
    pub items: Vec<ox_store::PinboardItem>,
    pub next_cursor: Option<String>,
}

#[derive(ToSchema)]
#[allow(dead_code)]
pub struct SavedPatternResponsePage {
    pub items: Vec<query::SavedPatternResponse>,
    pub next_cursor: Option<String>,
}

#[derive(ToSchema)]
#[allow(dead_code)]
pub struct KnowledgeEntryPage {
    pub items: Vec<ox_store::KnowledgeEntry>,
    pub next_cursor: Option<String>,
}

#[derive(ToSchema)]
#[allow(dead_code)]
pub struct OntologyDraftSummaryPage {
    pub items: Vec<ox_store::OntologyDraftSummary>,
    pub next_cursor: Option<String>,
}

/// Cursor-paginated evaluation run page. The runtime envelope is
/// `ApiResponse::page`, which the FE client unwraps to this shape.
#[derive(ToSchema)]
#[allow(dead_code)]
pub struct EvaluationRunPage {
    pub items: Vec<ox_store::evaluation::EvaluationRun>,
    pub next_cursor: Option<String>,
}

/// Cursor-paginated evaluation dataset summary page. Mirrors the
/// frontend-facing page shape after the universal API envelope is
/// unwrapped.
#[derive(ToSchema)]
#[allow(dead_code)]
pub struct EvaluationDatasetSummaryPage {
    pub items: Vec<ox_store::evaluation::EvaluationDatasetSummary>,
    pub next_cursor: Option<String>,
}
