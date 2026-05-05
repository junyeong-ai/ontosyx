use serde::Serialize;
use utoipa::openapi::security::{ApiKey, ApiKeyValue, SecurityScheme};
use utoipa::{Modify, OpenApi, ToSchema};

use crate::routes::{
    acl, approvals, audit, auth, chat, config, dashboards, federation_admin, governance_audit,
    governance_routing, health, insights, knowledge, lineage, load, models, notifications,
    ontology, perspectives, pins, prompts_admin, quality, query, recipes, reports, schedules,
    sessions, usage, users, workspaces,
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
    /// Machine-readable error type (e.g., "not_found", "bad_request")
    pub r#type: String,
    /// Human-readable error message
    pub message: String,
    /// Optional additional details
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
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
            Error responses use a separate envelope: `{ \"error\": { \"type\": \"...\", \"message\": \"...\" } }` \
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
        (name = "Ontologies", description = "Ontology management and export"),
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
        // Ontology drafts — lifecycle
        lifecycle::create_ontology_draft,
        lifecycle::list_ontology_drafts,
        lifecycle::get_ontology_draft,
        lifecycle::delete_ontology_draft,
        lifecycle::complete_ontology_draft,
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
        // Ontology
        ontology::create_ontology,
        ontology::get_workspace_ontology_detail,
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
        ontology::suggest_glossary_bindings,
        ontology::suggest_glossary_terms_for_property,
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
        load::list_prompts,
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
        // Models
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
            // Query
            query::ExecuteRawQueryRequest,
            query::ExecuteRawQueryResponse,
            query::SearchGraphRequest,
            query::ExpandGraphRequest,
            ox_ontology::graph_exploration::SearchResultNode,
            ox_ontology::graph_exploration::ExpandNeighbor,
            ox_ontology::graph_exploration::NodeExpansion,
            ox_ontology::graph_exploration::LabelStat,
            ox_ontology::graph_exploration::RelationshipPattern,
            ox_ontology::graph_exploration::PropertySchema,
            ox_ontology::graph_exploration::GraphSchemaOverview,
            // Ontology drafts
            types::CreateOntologyDraftRequest,
            types::OntologyDraftOrigin,
            types::DataSourceSpec,
            types::OntologyDraftView,
            types::AnalysisReportStatus,
            types::UpdateOntologyDraftDecisionsRequest,
            types::DesignOntologyDraftRequest,
            types::DesignOntologyDraftResponse,
            types::ReanalyzeOntologyDraftRequest,
            types::ReanalyzeOntologyDraftResponse,
            analysis::ReanalyzeModeledOntologyDraftRequest,
            types::RefineOntologyDraftRequest,
            types::RefineOntologyDraftResponse,
            types::ReconcileOntologyDraftRequest,
            types::ExtendOntologyDraftRequest,
            types::ExtendOntologyDraftResponse,
            types::CompleteOntologyDraftRequest,
            types::EditOntologyDraftRequest,
            types::EditOntologyDraftResponse,
            preview::PreviewSourceRequest,
            preview::PreviewSourceResponse,
            preview::PreviewTableSummary,
            scope::IncludeScopeTablesRequest,
            scope::DeferScopeTablesRequest,
            scope::ScopeUpdateResponse,
            // Ontology
            ontology::CreateOntologyRequest,
            ontology::CreateOntologyResponse,
            ontology::ApplyOntologyCommandsRequest,
            ontology::ApplyOntologyCommandsResponse,
            ontology::ImportOntologyRequest,
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
            ontology::SuggestTermsRequest,
            ontology::SuggestTermsResponse,
            ontology::PropertyCandidate,
            ontology::TermCandidate,
            ontology::TypeCandidate,
            ontology::VerifyElementRequest,
            ontology::VerifyElementResponse,
            ox_ontology::binding_suggestions::BindingSignal,
            ox_ontology::SchemaDependencyGraph,
            ox_ontology::DependencyBucket,
            ox_ontology::DependencyEdge,
            ox_ontology::DependencyKind,
            ox_ontology::SchemaEntityRef,
            // Pins
            pins::CreatePinRequest,
            // Perspectives
            perspectives::UpsertPerspectiveRequest,
            perspectives::PerspectiveFindParams,
            // Config
            config::ConfigEntry,
            config::UiConfig,
            config::UpdateConfigRequest,
            config::ConfigUpdate,
            // Load
            load::GenerateLoadPlanRequest,
            load::GenerateLoadPlanResponse,
            load::ExecuteLoadRequest,
            load::ExecuteLoadResponse,
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
            ox_store::DashboardWidget,
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
            ox_store::AuditRecord,
            AuditRecordPage,
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
            notifications::WebhookChannelConfig,
            notifications::CreateChannelRequest,
            notifications::UpdateChannelRequest,
            notifications::TestChannelResponse,
            // Schedules
            schedules::CreateScheduleRequest,
            schedules::UpdateScheduleRequest,
            // Reports
            reports::CreateReportRequest,
            reports::UpdateReportRequest,
            reports::ReportParameter,
            // Sessions
            sessions::ToolRespondRequest,
            sessions::ToolRespondResponse,
            // ACL
            acl::CreatePolicyRequest,
            acl::UpdatePolicyRequest,
            // Recipes
            recipes::CreateRecipeRequest,
            recipes::RecipeStatusUpdateRequest,
            // Knowledge
            knowledge::CreateKnowledgeEntryRequest,
            knowledge::UpdateKnowledgeEntryRequest,
            knowledge::UpdateKnowledgeStatusRequest,
            knowledge::BulkReviewApprovalsRequest,
            knowledge::KnowledgeBulkReviewResponse,
            knowledge::KnowledgeStats,
            // Models
            models::TestModelRequest,
            models::TestModelResponse,
            // Quality
            quality::CreateRuleRequest,
            quality::UpdateRuleRequest,
            quality::DecideStaleProposalRequest,
            quality::BulkDecideStaleProposalsRequest,
            quality::BulkDecideStaleProposalsResponse,
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
    pub variables: serde_json::Value,
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
