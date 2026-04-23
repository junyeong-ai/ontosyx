use serde::Serialize;
use utoipa::openapi::security::{ApiKey, ApiKeyValue, SecurityScheme};
use utoipa::{Modify, OpenApi, ToSchema};

use crate::routes::{
    chat, config, federation_admin, health, load, ontology, perspectives, pins, prompts_admin,
    query,
};

// Module aliases for utoipa path resolution — utoipa generates hidden __path_*
// structs in the module where #[utoipa::path] is applied, so we must reference
// the actual defining module, not the re-export.
use crate::routes::projects::analysis as project_analysis;
use crate::routes::projects::decisions as project_decisions;
use crate::routes::projects::edit as project_edit;
use crate::routes::projects::extend as project_extend;
use crate::routes::projects::lifecycle as project_lifecycle;
use crate::routes::projects::refinement as project_refinement;
use crate::routes::projects::revisions as project_revisions;
use crate::routes::projects::streaming as project_streaming;
use crate::routes::projects::types as project_types;

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
        version = "1.0.0",
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
        (name = "Chat", description = "Natural language query pipeline"),
        (name = "Query", description = "Raw query execution and history"),
        (name = "Projects", description = "Design project lifecycle management"),
        (name = "Ontologies", description = "Ontology management and export"),
        (name = "Pins", description = "Pinboard — saved query results"),
        (name = "Perspectives", description = "Workbench canvas perspectives"),
        (name = "Config", description = "System configuration"),
        (name = "Load", description = "Data loading into graph database"),
        (name = "System", description = "System administration"),
        (name = "Admin", description = "Platform administration (admin role required)"),
    ),
    paths(
        // Health
        health::health_check,
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
        // Projects — lifecycle
        project_lifecycle::create_project,
        project_lifecycle::list_projects,
        project_lifecycle::get_project,
        project_lifecycle::delete_project,
        project_lifecycle::complete_project,
        project_decisions::update_decisions,
        project_refinement::design_project,
        project_refinement::refine_project,
        project_refinement::apply_reconcile,
        project_analysis::reanalyze_project,
        project_edit::edit_project,
        project_extend::extend_project,
        project_streaming::design_project_stream,
        project_streaming::refine_project_stream,
        // Projects — revisions
        project_revisions::list_revisions,
        project_revisions::get_revision,
        project_revisions::restore_revision,
        // Ontologies
        ontology::list_ontologies,
        ontology::get_ontology_detail,
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
        ontology::suggest_glossary_bindings,
        ontology::suggest_glossary_terms_for_property,
        // Pins
        pins::create_pin,
        pins::list_pins,
        pins::delete_pin,
        // Perspectives
        perspectives::save_perspective,
        perspectives::list_perspectives,
        perspectives::get_default_perspective,
        perspectives::get_best_perspective,
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
            // Chat
            chat::ChatStreamRequest,
            // Query
            query::QueryRawRequest,
            query::QueryRawResponse,
            // Projects
            project_types::CreateProjectRequest,
            project_types::ProjectOrigin,
            project_types::ProjectSource,
            project_types::UpdateDecisionsRequest,
            project_types::ProjectDesignRequest,
            project_types::ProjectDesignResponse,
            project_types::ProjectReanalyzeRequest,
            project_types::ProjectReanalyzeResponse,
            project_types::ProjectRefineRequest,
            project_types::ProjectRefineResponse,
            project_types::ProjectReconcileRequest,
            project_types::ProjectExtendRequest,
            project_types::ProjectExtendResponse,
            project_types::ProjectCompleteRequest,
            project_types::ProjectEditRequest,
            project_types::ProjectEditResponse,
            // Ontology
            ontology::OntologyCommandsRequest,
            ontology::OntologyCommandsResponse,
            ontology::OntologyImportRequest,
            ontology::ProposeValueSetsRequest,
            ontology::ProposeValueSetsResponse,
            ontology::ProposePolicyBody,
            ontology::ProposalBody,
            ontology::EvidenceBody,
            ontology::SkipBody,
            ontology::BindingPolicyBody,
            ontology::SuggestBindingsRequest,
            ontology::SuggestBindingsResponse,
            ontology::SuggestTermsRequest,
            ontology::SuggestTermsResponse,
            ontology::PropertyCandidateBody,
            ontology::TermCandidateBody,
            ontology::SignalBody,
            // Pins
            pins::PinCreateRequest,
            // Perspectives
            perspectives::PerspectiveUpsertRequest,
            perspectives::PerspectiveFindParams,
            // Config
            config::ConfigEntry,
            config::UiConfig,
            config::ConfigUpdateRequest,
            config::ConfigUpdate,
            // Load
            load::LoadPlanRequest,
            load::LoadPlanResponse,
            load::LoadExecuteRequest,
            load::LoadExecuteResponse,
            load::PromptInfo,
            // Revisions
            project_revisions::ProjectRestoreResponse,
            // Admin — prompt templates
            prompts_admin::PromptCreateRequest,
            prompts_admin::PromptUpdateRequest,
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
            DesignProject,
            DesignProjectSummary,
            ontology::OntologyListItem,
            ontology::OntologyDetail,
            ontology::CurrentVersionSummary,
            QueryExecution,
            QueryExecutionSummary,
            PinboardItem,
            WorkbenchPerspective,
            OntologySnapshot,
            OntologySnapshotSummary,
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
#[derive(ToSchema)]
#[schema(as = DesignProject)]
#[allow(dead_code)]
pub struct DesignProject {
    pub id: uuid::Uuid,
    pub status: String,
    pub revision: i32,
    pub user_id: String,
    pub title: Option<String>,
    pub source_config: serde_json::Value,
    /// Canonical source identity (`{source_type}:{fingerprint}`)
    /// derived from `source_config` at project creation / reanalysis.
    pub source_id: String,
    pub source_data: Option<String>,
    pub source_schema: Option<serde_json::Value>,
    pub source_profile: Option<serde_json::Value>,
    pub analysis_report: Option<serde_json::Value>,
    pub design_options: serde_json::Value,
    pub source_mapping: Option<serde_json::Value>,
    pub ontology: Option<serde_json::Value>,
    pub quality_report: Option<serde_json::Value>,
    pub ontology_id: Option<uuid::Uuid>,
    pub source_history: serde_json::Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub analyzed_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Design project summary (lightweight, for list endpoints).
#[derive(ToSchema)]
#[schema(as = DesignProjectSummary)]
#[allow(dead_code)]
pub struct DesignProjectSummary {
    pub id: uuid::Uuid,
    pub status: String,
    pub revision: i32,
    pub user_id: String,
    pub title: Option<String>,
    pub source_config: serde_json::Value,
    pub ontology_id: Option<uuid::Uuid>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub analyzed_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Query execution record.
#[derive(ToSchema)]
#[schema(as = QueryExecution)]
#[allow(dead_code)]
pub struct QueryExecution {
    pub id: uuid::Uuid,
    pub user_id: String,
    pub question: String,
    pub ontology_lineage_id: String,
    pub ontology_version: i32,
    pub ontology_id: Option<uuid::Uuid>,
    pub ontology_snapshot: Option<serde_json::Value>,
    pub query_ir: serde_json::Value,
    pub compiled_target: String,
    pub compiled_query: String,
    pub results: serde_json::Value,
    pub widget: Option<serde_json::Value>,
    pub explanation: String,
    pub model: String,
    pub execution_time_ms: i64,
    pub query_bindings: Option<serde_json::Value>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Query execution summary (lightweight, for list endpoints).
#[derive(ToSchema)]
#[schema(as = QueryExecutionSummary)]
#[allow(dead_code)]
pub struct QueryExecutionSummary {
    pub id: uuid::Uuid,
    pub question: String,
    pub ontology_lineage_id: String,
    pub ontology_version: i32,
    pub compiled_target: String,
    pub model: String,
    pub execution_time_ms: i64,
    pub row_count: i64,
    pub has_widget: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Pinboard item — a saved query result.
#[derive(ToSchema)]
#[schema(as = PinboardItem)]
#[allow(dead_code)]
pub struct PinboardItem {
    pub id: uuid::Uuid,
    pub query_execution_id: uuid::Uuid,
    pub user_id: String,
    pub widget_spec: serde_json::Value,
    pub title: Option<String>,
    pub pinned_at: chrono::DateTime<chrono::Utc>,
}

/// Workbench perspective — saved canvas state.
#[derive(ToSchema)]
#[schema(as = WorkbenchPerspective)]
#[allow(dead_code)]
pub struct WorkbenchPerspective {
    pub id: uuid::Uuid,
    pub user_id: String,
    pub lineage_id: String,
    pub topology_signature: String,
    pub project_id: Option<uuid::Uuid>,
    pub name: String,
    pub positions: serde_json::Value,
    pub viewport: serde_json::Value,
    pub filters: serde_json::Value,
    pub collapsed_groups: serde_json::Value,
    pub is_default: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Ontology revision snapshot.
#[derive(ToSchema)]
#[schema(as = OntologySnapshot)]
#[allow(dead_code)]
pub struct OntologySnapshot {
    pub id: uuid::Uuid,
    pub project_id: uuid::Uuid,
    pub revision: i32,
    pub ontology: serde_json::Value,
    pub source_mapping: Option<serde_json::Value>,
    pub quality_report: Option<serde_json::Value>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Ontology revision snapshot summary (lightweight).
#[derive(ToSchema)]
#[schema(as = OntologySnapshotSummary)]
#[allow(dead_code)]
pub struct OntologySnapshotSummary {
    pub id: uuid::Uuid,
    pub revision: i32,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub node_count: i64,
    pub edge_count: i64,
}

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
