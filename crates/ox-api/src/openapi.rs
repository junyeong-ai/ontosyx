use serde::Serialize;
use utoipa::openapi::security::{ApiKey, ApiKeyValue, SecurityScheme};
use utoipa::{Modify, OpenApi, ToSchema};

use crate::routes::{chat, config, health, load, ontology, perspectives, pins, query};

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
        description = "The Semantic Orchestrator — Knowledge Graph Lifecycle Platform",
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
    ),
    paths(
        // Health
        health::health_check,
        // Chat
        chat::chat_stream,
        // Query
        query::raw_query,
        query::execute_from_ir,
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
    ),
    components(
        schemas(
            ErrorResponse,
            ErrorBody,
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
            // Store models
            CursorParams,
            DesignProject,
            DesignProjectSummary,
            SavedOntology,
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
    pub source_data: Option<String>,
    pub source_schema: Option<serde_json::Value>,
    pub source_profile: Option<serde_json::Value>,
    pub analysis_report: Option<serde_json::Value>,
    pub design_options: serde_json::Value,
    pub source_mapping: Option<serde_json::Value>,
    pub ontology: Option<serde_json::Value>,
    pub quality_report: Option<serde_json::Value>,
    pub saved_ontology_id: Option<uuid::Uuid>,
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
    pub saved_ontology_id: Option<uuid::Uuid>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub analyzed_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Saved ontology — a completed, frozen ontology.
#[derive(ToSchema)]
#[schema(as = SavedOntology)]
#[allow(dead_code)]
pub struct SavedOntology {
    pub id: uuid::Uuid,
    pub name: String,
    pub description: Option<String>,
    pub version: i32,
    pub ontology_ir: serde_json::Value,
    pub created_by: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Query execution record.
#[derive(ToSchema)]
#[schema(as = QueryExecution)]
#[allow(dead_code)]
pub struct QueryExecution {
    pub id: uuid::Uuid,
    pub user_id: String,
    pub question: String,
    pub ontology_id: String,
    pub ontology_version: i32,
    pub saved_ontology_id: Option<uuid::Uuid>,
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
    pub ontology_id: String,
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
