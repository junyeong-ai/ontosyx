use axum::{
    Router, middleware,
    routing::{delete, get, patch, post, put},
};

use crate::middleware::{require_auth, workspace_context};
use crate::state::AppState;

pub mod acl;
pub mod approvals;
pub mod audit;
pub mod auth;
pub mod chat;
pub mod config;
pub mod dashboards;
pub mod health;
pub mod lineage;
pub mod load;
pub mod ontology;
pub mod perspectives;
pub mod pins;
pub mod projects;
pub mod prompts_admin;
pub mod models;
pub mod quality;
pub mod query;
pub mod recipes;
pub mod reports;
pub mod schedules;
pub mod sessions;
pub mod usage;
pub mod users;
pub mod workspaces;
mod ws;

pub fn router(state: AppState) -> Router {
    // Public routes (no auth required)
    let public = Router::new()
        .route("/health", get(health::health_check))
        .route("/config/ui", get(config::get_ui_config))
        .route("/auth/token", post(auth::create_token));

    // Protected routes (require JWT or API key)
    let protected = Router::new()
        // Auth: current user info
        .route("/auth/me", get(auth::me))
        // Design projects — ontology design lifecycle
        .route("/projects", post(projects::create_project))
        .route("/projects", get(projects::list_projects))
        .route("/projects/{id}", get(projects::get_project))
        .route("/projects/{id}", delete(projects::delete_project))
        .route(
            "/projects/{id}/decisions",
            patch(projects::update_decisions),
        )
        .route("/projects/{id}/design", post(projects::design_project))
        .route(
            "/projects/{id}/design/stream",
            post(projects::design_project_stream),
        )
        .route(
            "/projects/{id}/reanalyze",
            post(projects::reanalyze_project),
        )
        .route("/projects/{id}/refine", post(projects::refine_project))
        .route(
            "/projects/{id}/refine/stream",
            post(projects::refine_project_stream),
        )
        .route(
            "/projects/{id}/apply-reconcile",
            post(projects::apply_reconcile),
        )
        .route("/projects/{id}/edit", post(projects::edit_project))
        .route("/projects/{id}/extend", post(projects::extend_project))
        .route("/projects/{id}/complete", post(projects::complete_project))
        .route(
            "/projects/{id}/deploy-schema",
            post(projects::deploy_schema),
        )
        .route(
            "/projects/{id}/load-plan",
            post(projects::generate_load_plan),
        )
        .route(
            "/projects/{id}/load/compile",
            post(projects::compile_load),
        )
        .route(
            "/projects/{id}/load/execute",
            post(projects::execute_load_from_source),
        )
        .route(
            "/projects/{id}/ontology",
            patch(ontology::apply_ontology_commands),
        )
        // Ontology revision history
        .route(
            "/projects/{id}/revisions",
            get(projects::list_revisions),
        )
        .route(
            "/projects/{id}/revisions/{rev}",
            get(projects::get_revision),
        )
        .route(
            "/projects/{id}/revisions/{rev}/restore",
            post(projects::restore_revision),
        )
        // Ontology revision diff
        .route(
            "/projects/{id}/revisions/{rev1}/diff/{rev2}",
            get(projects::diff_revisions),
        )
        .route(
            "/projects/{id}/diff/current",
            get(projects::diff_current),
        )
        .route(
            "/projects/{id}/revisions/{rev}/migrate",
            post(projects::migrate_schema),
        )
        // Ontology management
        .route("/ontologies", get(ontology::list_ontologies))
        // Ontology import/export (stateless transforms)
        .route("/ontology/normalize", post(ontology::normalize_ontology))
        .route("/ontology/export", post(ontology::export_ontology))
        .route(
            "/ontology/export/cypher",
            post(ontology::export_cypher),
        )
        .route(
            "/ontology/export/mermaid",
            post(ontology::export_mermaid),
        )
        .route(
            "/ontology/export/graphql",
            post(ontology::export_graphql),
        )
        .route(
            "/ontology/export/owl",
            post(ontology::export_owl),
        )
        .route(
            "/ontology/export/shacl",
            post(ontology::export_shacl),
        )
        .route(
            "/ontology/export/typescript",
            post(ontology::export_typescript),
        )
        .route(
            "/ontology/export/python",
            post(ontology::export_python),
        )
        // Ontology import
        .route(
            "/ontology/import/owl",
            post(ontology::import_owl),
        )
        // Ontology insight suggestions
        .route(
            "/ontology/suggestions",
            post(ontology::suggest_insights),
        )
        // Data loading
        .route("/load", post(load::plan_load))
        .route("/load/execute", post(load::execute_load))
        // System
        .route("/prompts", get(load::list_prompts))
        // Config management
        .route("/config", get(config::get_config))
        .route("/config", patch(config::update_config))
        // Chat: unified AI pipeline (intent → query/edit/explain)
        .route("/chat/stream", post(chat::chat_stream))
        // Query execution history
        .route("/query/history", get(query::list_executions))
        .route("/query/history/{id}", get(query::get_execution))
        .route(
            "/query/history/{id}/feedback",
            patch(query::set_feedback),
        )
        // Raw query
        .route("/query/raw", post(query::raw_query))
        // QueryIR-based query (visual query builder)
        .route("/query/from-ir", post(query::execute_from_ir))
        // Graph search & exploration
        .route("/search", post(query::search_graph))
        .route("/search/expand", post(query::expand_node))
        // Graph metadata
        .route("/graph/overview", get(query::graph_overview))
        // User management
        .route("/users", get(users::list_users))
        .route("/users/{id}/role", patch(users::update_user_role))
        // Analysis recipes
        .route("/recipes", post(recipes::create_recipe))
        .route("/recipes", get(recipes::list_recipes))
        .route("/recipes/{id}", get(recipes::get_recipe))
        .route("/recipes/{id}", delete(recipes::delete_recipe))
        .route(
            "/recipes/{id}/status",
            patch(recipes::update_recipe_status),
        )
        .route(
            "/recipes/{id}/versions",
            post(recipes::create_recipe_version),
        )
        .route(
            "/recipes/{id}/versions",
            get(recipes::list_recipe_versions),
        )
        .route(
            "/recipes/{id}/results",
            get(recipes::list_recipe_results),
        )
        .route(
            "/recipes/{id}/schedule",
            post(schedules::create_schedule),
        )
        // Scheduled tasks
        .route(
            "/scheduled-tasks",
            get(schedules::list_schedules),
        )
        .route(
            "/scheduled-tasks/{id}",
            get(schedules::get_schedule),
        )
        .route(
            "/scheduled-tasks/{id}",
            patch(schedules::update_schedule).delete(schedules::delete_schedule),
        )
        // Dashboards
        .route("/dashboards", post(dashboards::create_dashboard))
        .route("/dashboards", get(dashboards::list_dashboards))
        .route("/dashboards/{id}", get(dashboards::get_dashboard))
        .route("/dashboards/{id}", patch(dashboards::update_dashboard))
        .route("/dashboards/{id}", delete(dashboards::delete_dashboard))
        .route(
            "/dashboards/{id}/widgets",
            post(dashboards::add_widget),
        )
        .route(
            "/dashboards/{id}/widgets",
            get(dashboards::list_widgets),
        )
        .route(
            "/dashboards/{id}/widgets/{widget_id}",
            patch(dashboards::update_widget).delete(dashboards::delete_widget),
        )
        // Saved Reports
        .route("/reports", post(reports::create_report))
        .route("/reports", get(reports::list_reports))
        .route("/reports/{id}", get(reports::get_report))
        .route("/reports/{id}", patch(reports::update_report))
        .route("/reports/{id}", delete(reports::delete_report))
        .route(
            "/reports/{id}/execute",
            post(reports::execute_report),
        )
        // Pinboard
        .route("/pins", post(pins::create_pin))
        .route("/pins", get(pins::list_pins))
        .route("/pins/{id}", delete(pins::delete_pin))
        // Perspectives
        .route("/perspectives", put(perspectives::save_perspective))
        .route(
            "/perspectives/by-lineage/{lineage_id}",
            get(perspectives::list_perspectives),
        )
        .route(
            "/perspectives/by-lineage/{lineage_id}/default",
            get(perspectives::get_default_perspective),
        )
        .route(
            "/perspectives/by-lineage/{lineage_id}/best",
            get(perspectives::get_best_perspective),
        )
        .route(
            "/perspectives/{id}",
            delete(perspectives::delete_perspective),
        )
        // Admin: prompt template management
        .route("/admin/prompts", get(prompts_admin::list_prompt_templates))
        .route("/admin/prompts", post(prompts_admin::create_prompt_template))
        .route(
            "/admin/prompts/{id}",
            get(prompts_admin::get_prompt_template),
        )
        .route(
            "/admin/prompts/{id}",
            patch(prompts_admin::update_prompt_template),
        )
        // Ontology verifications
        .route(
            "/ontology/{id}/verifications",
            post(ontology::verify_element).get(ontology::list_verifications),
        )
        .route(
            "/ontology/{id}/verifications/{element_id}",
            delete(ontology::delete_verification),
        )
        // Ontology schema re-indexing + audit
        .route(
            "/ontology/{id}/reindex",
            post(ontology::reindex_schema),
        )
        .route(
            "/ontology/{id}/audit",
            post(ontology::audit_graph),
        )
        .route(
            "/ontology/adopt-graph",
            post(ontology::adopt_graph),
        )
        // Agent sessions (audit)
        .route("/sessions", get(sessions::list_sessions))
        .route("/sessions/{id}", get(sessions::get_session))
        .route(
            "/sessions/{id}/events",
            get(sessions::list_session_events),
        )
        .route(
            "/sessions/{id}/messages",
            get(sessions::get_session_messages),
        )
        // HITL tool review
        .route(
            "/sessions/{session_id}/tools/{tool_id}/respond",
            post(sessions::respond_tool_review),
        )
        // Approval workflows
        .route("/approvals", get(approvals::list_approvals))
        .route("/approvals/{id}", get(approvals::get_approval))
        .route(
            "/approvals/{id}/review",
            post(approvals::review_approval),
        )
        // Audit trail
        .route("/audit", get(audit::list_audit_events))
        // Usage metering
        .route("/usage", get(usage::get_usage_summary))
        // Data lineage
        .route("/lineage", get(lineage::get_lineage_summary))
        .route(
            "/lineage/label/{label}",
            get(lineage::get_lineage_for_label),
        )
        .route(
            "/lineage/project/{id}",
            get(lineage::get_lineage_for_project),
        )
        // Quality rules
        .route("/quality/rules", post(quality::create_rule))
        .route("/quality/rules", get(quality::list_rules))
        .route("/quality/rules/{id}", get(quality::get_rule))
        .route("/quality/rules/{id}", patch(quality::update_rule))
        .route("/quality/rules/{id}", delete(quality::delete_rule))
        .route("/quality/dashboard", get(quality::quality_dashboard))
        .route(
            "/quality/rules/{id}/results",
            get(quality::rule_results),
        )
        // Model configs
        .route("/models/configs", get(models::list_model_configs))
        .route("/models/configs", post(models::create_model_config))
        .route(
            "/models/configs/{id}",
            patch(models::update_model_config).delete(models::delete_model_config),
        )
        // Model routing rules
        .route("/models/routing-rules", get(models::list_routing_rules))
        .route(
            "/models/routing-rules",
            post(models::create_routing_rule),
        )
        .route(
            "/models/routing-rules/{id}",
            patch(models::update_routing_rule).delete(models::delete_routing_rule),
        )
        // Model connection test
        .route("/models/test", post(models::test_model_connection))
        // ACL policies
        .route("/acl/policies", post(acl::create_policy))
        .route("/acl/policies", get(acl::list_policies))
        .route("/acl/policies/{id}", get(acl::get_policy))
        .route("/acl/policies/{id}", patch(acl::update_policy))
        .route("/acl/policies/{id}", delete(acl::delete_policy))
        .route("/acl/effective", get(acl::effective_policies))
        // Workspaces
        .route("/workspaces", post(workspaces::create_workspace))
        .route("/workspaces", get(workspaces::list_workspaces))
        .route("/workspaces/{id}", get(workspaces::get_workspace))
        .route("/workspaces/{id}", patch(workspaces::update_workspace))
        .route("/workspaces/{id}", delete(workspaces::delete_workspace))
        .route(
            "/workspaces/{id}/members",
            post(workspaces::add_member).get(workspaces::list_members),
        )
        .route(
            "/workspaces/{id}/members/{uid}",
            patch(workspaces::update_member_role).delete(workspaces::remove_member),
        )
        // Middleware order (outer → inner): require_auth → workspace_context → audit_log
        // route_layer applies bottom-up, so audit_log (innermost) is first
        .route_layer(middleware::from_fn_with_state(state.clone(), crate::audit_middleware::audit_log))
        .route_layer(middleware::from_fn_with_state(state.clone(), workspace_context))
        .route_layer(middleware::from_fn_with_state(state.clone(), require_auth));

    // WebSocket routes (auth via query param, not middleware)
    let ws_routes = Router::new()
        .route("/ws/collab", get(ws::collab_ws));

    public
        .merge(protected)
        .merge(ws_routes)
        .with_state(state)
}
