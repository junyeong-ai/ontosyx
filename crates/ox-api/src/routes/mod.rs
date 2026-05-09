use axum::{
    Router, middleware,
    routing::{delete, get, patch, post, put},
};

use crate::middleware::{require_auth, workspace_context};
use crate::state::AppState;

pub mod acl;
pub mod ambiguity;
pub mod approvals;
pub mod audit;
pub mod auth;
pub mod chat;
pub mod community_summaries;
pub mod config;
pub mod dashboards;
pub mod evaluation;
pub mod evaluation_retrieval;
pub mod federation_admin;
pub mod governance_audit;
pub mod governance_routing;
pub mod health;
pub mod insights;
pub mod knowledge;
pub mod lineage;
pub mod load;
pub mod models;
pub mod notifications;
pub mod ontology;
pub mod ontology_drafts;
pub mod perspectives;
pub mod pins;
pub mod prompts_admin;
pub mod quality;
pub mod query;
pub mod recipes;
pub mod reports;
pub mod schedules;
pub mod sessions;
pub mod sources;
pub mod usage;
pub mod users;
pub mod verified_queries;
pub mod workspaces;
mod ws;

pub fn router(state: AppState) -> Router {
    // Public routes (no auth required)
    let public = Router::new()
        .route("/health", get(health::health_check))
        // Industry-standard liveness/readiness probe — flat shape,
        // no envelope, no auth. Used by k8s probes, Datadog,
        // Prometheus, and ops scripts. /api/health stays wrapped
        // for the FE admin page; the two share body construction
        // so they cannot drift.
        .route("/healthz", get(health::healthz))
        .route("/config/ui", get(config::get_ui_config))
        .route("/auth/token", post(auth::create_token))
        .route(
            "/shared/dashboards/{token}",
            get(dashboards::get_shared_dashboard),
        );

    // Protected routes (require JWT or API key)
    let protected = Router::new()
        // Auth: current user info
        .route("/auth/me", get(auth::me))
        // Auth: short-lived JWT for the collaboration WebSocket
        .route("/auth/ws-token", get(auth::ws_token))
        // Auth: revoke caller's current JWT
        .route("/auth/logout", post(auth::logout))
        // Design projects — ontology design lifecycle
        .route(
            "/ontology-drafts",
            post(ontology_drafts::create_ontology_draft),
        )
        .route(
            "/ontology-drafts",
            get(ontology_drafts::list_ontology_drafts),
        )
        .route(
            "/ontology-drafts/source-preview",
            post(ontology_drafts::preview_source),
        )
        .route(
            "/ontology-drafts/{id}",
            get(ontology_drafts::get_ontology_draft),
        )
        .route(
            "/ontology-drafts/{id}",
            delete(ontology_drafts::delete_ontology_draft),
        )
        .route(
            "/ontology-drafts/{id}/decisions",
            patch(ontology_drafts::update_decisions),
        )
        .route(
            "/ontology-drafts/{id}/design",
            post(ontology_drafts::design_ontology_draft),
        )
        .route(
            "/ontology-drafts/{id}/design/stream",
            post(ontology_drafts::design_ontology_draft_stream),
        )
        .route(
            "/ontology-drafts/{id}/reanalyze",
            post(ontology_drafts::reanalyze_ontology_draft),
        )
        .route(
            "/ontology-drafts/{id}/reanalyze-modeled",
            post(ontology_drafts::reanalyze_modeled_ontology_draft),
        )
        .route(
            "/ontology-drafts/{id}/scope/include",
            post(ontology_drafts::include_scope_tables),
        )
        .route(
            "/ontology-drafts/{id}/scope/defer",
            post(ontology_drafts::defer_scope_tables),
        )
        .route(
            "/ontology-drafts/{id}/refine",
            post(ontology_drafts::refine_ontology_draft),
        )
        .route(
            "/ontology-drafts/{id}/refine/stream",
            post(ontology_drafts::refine_ontology_draft_stream),
        )
        .route(
            "/ontology-drafts/{id}/apply-reconcile",
            post(ontology_drafts::apply_reconcile),
        )
        .route(
            "/ontology-drafts/{id}/edit",
            post(ontology_drafts::edit_ontology_draft),
        )
        .route(
            "/ontology-drafts/{id}/extend",
            post(ontology_drafts::extend_ontology_draft),
        )
        .route(
            "/ontology-drafts/{id}/complete",
            post(ontology_drafts::complete_ontology_draft),
        )
        .route(
            "/ontology-drafts/{id}/deploy-schema",
            post(ontology_drafts::deploy_schema),
        )
        .route(
            "/ontology-drafts/{id}/load-plan",
            post(ontology_drafts::generate_load_plan),
        )
        .route(
            "/ontology-drafts/{id}/load/compile",
            post(ontology_drafts::compile_load),
        )
        .route(
            "/ontology-drafts/{id}/load/execute",
            post(ontology_drafts::execute_load_from_source),
        )
        .route(
            "/ontology-drafts/{id}/ontology",
            patch(ontology::apply_ontology_commands),
        )
        // Ontology revision history
        .route(
            "/ontology-drafts/{id}/revisions",
            get(ontology_drafts::list_revisions),
        )
        .route(
            "/ontology-drafts/{id}/revisions/{rev}",
            get(ontology_drafts::get_revision),
        )
        .route(
            "/ontology-drafts/{id}/revisions/{rev}/restore",
            post(ontology_drafts::restore_revision),
        )
        // Ontology revision diff
        .route(
            "/ontology-drafts/{id}/revisions/{rev1}/diff/{rev2}",
            get(ontology_drafts::diff_revisions),
        )
        .route(
            "/ontology-drafts/{id}/diff/current",
            get(ontology_drafts::diff_current),
        )
        .route(
            "/ontology-drafts/{id}/diff/canonical",
            get(ontology_drafts::diff_canonical),
        )
        .route(
            "/ontology-drafts/{id}/rebase/preview",
            get(ontology_drafts::rebase_preview),
        )
        .route(
            "/ontology-drafts/{id}/rebase",
            post(ontology_drafts::rebase_draft),
        )
        .route(
            "/ontology-drafts/{id}/revisions/{rev}/migrate",
            post(ontology_drafts::migrate_schema),
        )
        // Ontology management — workspace × ontology is 1:1, so the
        // singular path with no `{id}` segment is the canonical
        // surface. `GET /ontology` returns the workspace's canonical
        // ontology (404 when none yet); `POST /ontology` creates it
        // (409 when one already exists).
        .route(
            "/ontology",
            get(ontology::get_workspace_ontology_detail).post(ontology::create_ontology),
        )
        .route("/ontology/versions", get(ontology::list_canonical_versions))
        .route(
            "/ontology/type-candidates",
            get(ontology::list_type_candidates),
        )
        .route("/ontology/edits", post(ontology::apply_ontology_edits))
        .route("/ontology/map-summary", get(ontology::map_summary))
        .route("/ontology/axis-items", get(ontology::list_axis_items))
        .route("/ontology/cross-refs", get(ontology::list_cross_refs))
        .route(
            "/ontology/communities",
            get(community_summaries::list_community_summaries)
                .post(community_summaries::upsert_community_summary),
        )
        .route(
            "/ontology/communities/search",
            get(community_summaries::search_community_summaries),
        )
        .route(
            "/ontology/communities/{id}",
            delete(community_summaries::delete_community_summary),
        )
        .route(
            "/ontology/dependencies",
            get(ontology::get_ontology_dependencies),
        )
        .route("/ontology/validate", get(ontology::get_ontology_validate))
        .route("/ontology/enrich", post(ontology::enrich_ontology))
        .route(
            "/ontology/value-sets/propose",
            post(ontology::propose_ontology_value_sets),
        )
        .route(
            "/ontology/notation-patterns/propose",
            post(ontology::propose_ontology_notation_patterns),
        )
        .route(
            "/ontology/concepts/suggest-property-bindings",
            post(ontology::suggest_concept_property_bindings),
        )
        .route(
            "/ontology/properties/{owner_kind}/{owner_type_id}/{property_id}/suggest-concepts",
            post(ontology::suggest_concepts_for_property),
        )
        // Ontology import/export (stateless transforms)
        .route("/ontology/normalize", post(ontology::normalize_ontology))
        .route("/ontology/export", post(ontology::export_ontology))
        .route("/ontology/export/cypher", post(ontology::export_cypher))
        .route("/ontology/export/mermaid", post(ontology::export_mermaid))
        .route("/ontology/export/graphql", post(ontology::export_graphql))
        .route("/ontology/export/owl", post(ontology::export_owl))
        .route("/ontology/export/shacl", post(ontology::export_shacl))
        .route(
            "/ontology/export/typescript",
            post(ontology::export_typescript),
        )
        .route("/ontology/export/python", post(ontology::export_python))
        // Ontology import
        .route("/ontology/import/owl", post(ontology::import_owl))
        // Ontology insight suggestions
        .route("/ontology/suggestions", post(ontology::suggest_insights))
        // Data loading
        .route("/load", post(load::plan_load))
        .route("/load/execute", post(load::execute_load))
        .route("/load/checkpoints", get(load::list_load_checkpoints))
        .route(
            "/load/checkpoints/{id}",
            delete(load::delete_load_checkpoint),
        )
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
            patch(query::update_feedback),
        )
        // Raw query
        .route("/query/raw", post(query::raw_query))
        // QueryIR-based query (visual query builder)
        .route("/query/from-ir", post(query::execute_from_ir))
        // QueryIR-based query via the federation (VOL) path — bypasses
        // Cypher/Neo4j entirely, executes through DataFusion against
        // registered data-source adapters.
        .route(
            "/query/from-ir/federation",
            post(query::execute_from_ir_federation),
        )
        // PatternIR <-> QueryIR transforms (visual query builder)
        .route("/query/pattern/compile", post(query::compile_pattern))
        .route("/query/pattern/decompile", post(query::decompile_pattern))
        // Saved PatternIR — canvas layout persistence
        .route("/query/pattern/saved", post(query::create_saved_pattern))
        .route("/query/pattern/saved", get(query::list_saved_patterns))
        .route("/query/pattern/saved/{id}", get(query::get_saved_pattern))
        .route(
            "/query/pattern/saved/{id}",
            patch(query::update_saved_pattern),
        )
        .route(
            "/query/pattern/saved/{id}",
            delete(query::delete_saved_pattern),
        )
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
        .route("/recipes/{id}/status", patch(recipes::update_recipe_status))
        .route(
            "/recipes/{id}/versions",
            post(recipes::create_recipe_version),
        )
        .route("/recipes/{id}/versions", get(recipes::list_recipe_versions))
        .route("/recipes/{id}/results", get(recipes::list_recipe_results))
        .route("/recipes/{id}/schedule", post(schedules::create_schedule))
        // Knowledge base
        .route("/knowledge", post(knowledge::create_knowledge))
        .route("/knowledge", get(knowledge::list_knowledge))
        .route("/knowledge/stale", get(knowledge::list_stale))
        .route("/knowledge/stats", get(knowledge::knowledge_stats))
        .route("/knowledge/bulk-review", post(knowledge::bulk_review))
        .route("/knowledge/{id}", get(knowledge::get_knowledge))
        .route("/knowledge/{id}", patch(knowledge::update_knowledge))
        .route("/knowledge/{id}", delete(knowledge::delete_knowledge))
        .route("/knowledge/{id}/status", patch(knowledge::update_status))
        // Verified query bank (Φ11)
        .route(
            "/verified-queries",
            post(verified_queries::promote_verified_query)
                .get(verified_queries::list_verified_queries),
        )
        .route(
            "/verified-queries/{id}",
            get(verified_queries::get_verified_query)
                .delete(verified_queries::delete_verified_query),
        )
        .route(
            "/verified-queries/{id}/transition-status",
            post(verified_queries::transition_verified_query_status),
        )
        // Scheduled tasks
        .route("/scheduled-tasks", get(schedules::list_schedules))
        .route("/scheduled-tasks/{id}", get(schedules::get_schedule))
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
        .route("/dashboards/{id}/widgets", post(dashboards::add_widget))
        .route("/dashboards/{id}/widgets", get(dashboards::list_widgets))
        .route(
            "/dashboards/{id}/widgets/{widget_id}",
            patch(dashboards::update_widget).delete(dashboards::delete_widget),
        )
        .route(
            "/dashboards/{id}/share",
            post(dashboards::share_dashboard).delete(dashboards::unshare_dashboard),
        )
        // Saved Reports
        .route("/reports", post(reports::create_report))
        .route("/reports", get(reports::list_reports))
        .route("/reports/{id}", get(reports::get_report))
        .route("/reports/{id}", patch(reports::update_report))
        .route("/reports/{id}", delete(reports::delete_report))
        .route("/reports/{id}/execute", post(reports::execute_report))
        // Pinboard
        .route("/pins", post(pins::create_pin))
        .route("/pins", get(pins::list_pins))
        .route("/pins/{id}", delete(pins::delete_pin))
        // Persisted insights — first-class saved discoveries.
        .route("/insights", post(insights::create_insight))
        .route("/insights", get(insights::list_insights))
        .route("/insights/{id}", get(insights::get_insight))
        .route(
            "/insights/{id}",
            axum::routing::put(insights::update_insight),
        )
        .route("/insights/{id}", delete(insights::delete_insight))
        // Evaluation surface — RAGAS-style metric loop.
        .route("/evaluation/runs", post(evaluation::create_evaluation_run))
        .route("/evaluation/runs", get(evaluation::list_evaluation_runs))
        .route("/evaluation/runs/{id}", get(evaluation::get_evaluation_run))
        .route(
            "/evaluation/runs/{id}",
            delete(evaluation::delete_evaluation_run),
        )
        .route(
            "/evaluation/runs/{id}/complete",
            post(evaluation::complete_evaluation_run),
        )
        .route(
            "/evaluation/runs/{run_id}/cases",
            put(evaluation::upsert_evaluation_case),
        )
        .route(
            "/evaluation/runs/{run_id}/cases",
            get(evaluation::list_evaluation_cases),
        )
        .route(
            "/evaluation/runs/{run_id}/cases/{case_key}/execute",
            post(evaluation::execute_evaluation_case),
        )
        .route(
            "/evaluation/runs/{run_id}/cases/bulk",
            post(evaluation::bulk_upsert_evaluation_cases),
        )
        .route(
            "/evaluation/cases/{case_id}/metrics",
            put(evaluation::upsert_evaluation_metric),
        )
        .route(
            "/evaluation/cases/{case_id}/metrics",
            get(evaluation::list_evaluation_metrics),
        )
        .route(
            "/evaluation/cases/{case_id}/judge",
            post(evaluation::judge_evaluation_case),
        )
        .route(
            "/evaluation/cases/{case_id}/judge_safety",
            post(evaluation::judge_safety_evaluation_case),
        )
        .route(
            "/evaluation/cases/{case_id}/promote-to-dataset",
            post(evaluation::promote_case_to_dataset),
        )
        .route(
            "/evaluation/runs/{run_id}/summary",
            get(evaluation::evaluation_run_summary),
        )
        .route(
            "/evaluation/runs/{run_id}/comparison-outliers",
            get(evaluation::list_run_comparison_outliers),
        )
        .route(
            "/evaluation/settings",
            get(evaluation::get_evaluation_settings),
        )
        .route(
            "/evaluation/settings",
            put(evaluation::update_evaluation_settings),
        )
        // Evaluation datasets — frozen Q+expected pairs reusable
        // across runs (Phoenix / Braintrust / LangSmith pattern).
        .route(
            "/evaluation/datasets",
            post(evaluation::upsert_evaluation_dataset),
        )
        .route(
            "/evaluation/datasets",
            get(evaluation::list_evaluation_datasets),
        )
        .route(
            "/evaluation/datasets/{id}",
            get(evaluation::get_evaluation_dataset),
        )
        .route(
            "/evaluation/datasets/{id}",
            delete(evaluation::delete_evaluation_dataset),
        )
        .route(
            "/evaluation/datasets/{id}/items",
            get(evaluation::list_evaluation_dataset_items),
        )
        .route(
            "/evaluation/datasets/{id}/items",
            put(evaluation::replace_evaluation_dataset_items),
        )
        .route(
            "/evaluation/runs/from-dataset",
            post(evaluation::create_run_from_dataset),
        )
        .route(
            "/evaluation/runs/diff",
            get(evaluation::compare_evaluation_runs),
        )
        // Perspectives
        .route("/perspectives", put(perspectives::save_perspective))
        .route(
            "/perspectives/by-lineage/{lineage_id}",
            get(perspectives::list_perspectives),
        )
        .route(
            "/perspectives/by-lineage/{lineage_id}/default",
            get(perspectives::find_default_perspective),
        )
        .route(
            "/perspectives/by-lineage/{lineage_id}/best",
            get(perspectives::find_best_perspective),
        )
        .route(
            "/perspectives/{id}",
            delete(perspectives::delete_perspective),
        )
        // Admin: prompt template management
        .route("/admin/prompts", get(prompts_admin::list_prompt_templates))
        .route(
            "/admin/prompts",
            post(prompts_admin::create_prompt_template),
        )
        .route(
            "/admin/prompts/{id}",
            get(prompts_admin::get_prompt_template),
        )
        .route(
            "/admin/prompts/{id}",
            patch(prompts_admin::update_prompt_template)
                .delete(prompts_admin::delete_prompt_template),
        )
        // Admin: federation adapter registry (VOL query path)
        .route(
            "/admin/federation/adapters",
            get(federation_admin::list_adapters).post(federation_admin::register_adapter),
        )
        .route(
            "/admin/federation/adapters/preview",
            post(federation_admin::preview_adapter),
        )
        .route(
            "/admin/federation/adapters/refresh",
            post(federation_admin::refresh_adapters),
        )
        .route(
            "/admin/federation/health",
            get(federation_admin::federation_health),
        )
        // Admin CRUD for ChangeRoutingRule overrides. The runtime
        // resolution path is already DB-driven; these routes let an
        // admin edit the workspace's row through the UI rather than
        // hand-running SQL.
        .route(
            "/admin/governance/routing",
            get(governance_routing::list_routing_rules),
        )
        .route(
            "/admin/governance/routing/{change_type}",
            axum::routing::put(governance_routing::upsert_routing_rule)
                .delete(governance_routing::delete_routing_rule),
        )
        // Workspace-wide PROV-O audit trail — streams provenance
        // entities across every committed ontology in the workspace.
        .route(
            "/governance/audit",
            get(governance_audit::list_audit_records),
        )
        .route(
            "/admin/federation/adapters/{source_id}",
            get(federation_admin::get_adapter).delete(federation_admin::delete_adapter),
        )
        .route(
            "/admin/federation/adapters/{source_id}/tables",
            get(federation_admin::list_adapter_tables),
        )
        .route(
            "/admin/federation/adapters/{source_id}/analyze",
            post(federation_admin::analyze_adapter),
        )
        .route(
            "/admin/federation/adapters/{source_id}/analysis",
            get(federation_admin::get_adapter_analysis),
        )
        // Ontology verifications
        .route(
            "/ontology/verifications",
            post(ontology::verify_element).get(ontology::list_verifications),
        )
        .route(
            "/ontology/verifications/{element_id}",
            delete(ontology::delete_verification),
        )
        // Ontology schema re-indexing + audit
        .route("/ontology/reindex", post(ontology::reindex_schema))
        .route("/ontology/audit", post(ontology::graph_audit_report))
        .route("/ontology/adopt-graph", post(ontology::adopt_graph))
        // Agent sessions (audit)
        .route("/sessions", get(sessions::list_sessions))
        .route(
            "/sessions/{id}",
            get(sessions::get_session).delete(sessions::delete_session),
        )
        .route("/sessions/{id}/events", get(sessions::list_session_events))
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
        .route("/approvals/{id}/review", post(approvals::review_approval))
        .route(
            "/approvals/bulk-review",
            post(approvals::bulk_review_approvals),
        )
        .route(
            "/approvals/{id}/comments",
            get(approvals::list_approval_comments).post(approvals::create_approval_comment),
        )
        // Audit trail
        .route("/audit", get(audit::list_audit_events))
        // Usage metering
        .route("/usage", get(usage::get_usage_summary))
        // Data lineage
        .route("/lineage", get(lineage::get_lineage_summary))
        .route(
            "/lineage/label/{label}",
            get(lineage::list_lineage_for_label),
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
        // 6-창 ontology-quality metrics (signal-backed)
        .route("/quality/metrics", get(quality::get_quality_metrics))
        .route("/quality/baseline", get(quality::get_quality_baseline))
        .route("/quality/shacl-failures", get(quality::list_shacl_failures))
        .route("/quality/stale-types", get(quality::list_stale_types))
        .route(
            "/quality/stale-proposals",
            get(quality::list_stale_proposals),
        )
        .route(
            "/quality/stale-proposals/{id}",
            patch(quality::decide_stale_proposal),
        )
        .route(
            "/quality/stale-proposals/bulk-decide",
            post(quality::bulk_decide_stale_proposals),
        )
        .route("/quality/rules/{id}/results", get(quality::rule_results))
        .route("/quality/rules/{id}/execute", post(quality::execute_rule))
        .route("/quality/execute-all", post(quality::execute_all_rules))
        // Ambiguity admin — closed-loop resolver surface
        .route("/ambiguities", get(ambiguity::list_ambiguities))
        .route(
            "/ambiguities/bulk-revoke",
            post(ambiguity::bulk_revoke_ambiguities),
        )
        .route("/ambiguities/{id}", get(ambiguity::get_ambiguity))
        .route(
            "/ambiguities/{id}/resolve",
            post(ambiguity::resolve_ambiguity),
        )
        .route(
            "/ambiguities/{id}/revoke",
            post(ambiguity::revoke_ambiguity),
        )
        // Notification channels
        .route(
            "/notifications/channels",
            post(notifications::create_channel),
        )
        .route("/notifications/channels", get(notifications::list_channels))
        .route(
            "/notifications/channels/{id}",
            patch(notifications::update_channel).delete(notifications::delete_channel),
        )
        .route(
            "/notifications/channels/{id}/test",
            post(notifications::test_channel),
        )
        .route("/notifications/log", get(notifications::list_logs))
        // Model configs
        .route("/models/operations", get(models::list_model_operations))
        .route("/models/configs", get(models::list_model_configs))
        .route("/models/configs", post(models::create_model_config))
        .route(
            "/models/configs/{id}",
            patch(models::update_model_config).delete(models::delete_model_config),
        )
        // Model routing rules
        .route("/models/routing-rules", get(models::list_routing_rules))
        .route("/models/routing-rules", post(models::create_routing_rule))
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
        // Middleware order (outer → inner):
        //   require_auth → workspace_context → idempotency → audit_log → handler
        //
        // `route_layer` applies bottom-up, so the innermost wraps
        // first. Idempotency sits between workspace_context (it
        // needs `WorkspaceContext` from extensions) and audit_log
        // (so a replayed cache-hit response still flows through
        // the audit trail; the `idempotent-replay: true` header
        // distinguishes the row).
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            crate::audit_middleware::audit_log,
        ))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            crate::idempotency::idempotency_layer,
        ))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            workspace_context,
        ))
        .route_layer(middleware::from_fn_with_state(state.clone(), require_auth));

    // Auth-only routes: require authentication but NOT workspace context.
    // Used for bootstrap endpoints that must work before a workspace is selected.
    let auth_only = Router::new()
        .route("/workspaces", post(workspaces::create_workspace))
        .route("/workspaces", get(workspaces::list_workspaces))
        .route("/workspaces/{id}", get(workspaces::get_workspace))
        .route("/workspaces/{id}/members", get(workspaces::list_members))
        .route(
            "/sources/test-connection",
            post(sources::test_source_connection),
        )
        .route_layer(middleware::from_fn_with_state(state.clone(), require_auth));

    // Workspace management routes: require auth + workspace context for admin checks.
    let workspace_mgmt = Router::new()
        .route("/workspaces/me", get(workspaces::workspace_me))
        .route("/workspaces/{id}", patch(workspaces::update_workspace))
        .route("/workspaces/{id}", delete(workspaces::delete_workspace))
        .route(
            "/workspaces/{id}/locale",
            put(workspaces::update_workspace_locale),
        )
        .route("/workspaces/{id}/members", post(workspaces::add_member))
        .route(
            "/workspaces/{id}/members/{uid}",
            patch(workspaces::update_member_role).delete(workspaces::remove_member),
        )
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            workspace_context,
        ))
        .route_layer(middleware::from_fn_with_state(state.clone(), require_auth));

    // WebSocket upgrade routes — bypass `require_auth` /
    // `workspace_context` middleware because the WS protocol does
    // its own first-frame auth (`ClientMessage::Authenticate`) and
    // binds `WORKSPACE_ID` for the connection's lifetime inside
    // the handler. The wire types live on the OpenAPI surface as
    // schemas but no HTTP path is published here.
    let ws_routes = Router::new().route("/ws/collab", get(ws::collab_ws));

    public
        .merge(protected)
        .merge(auth_only)
        .merge(workspace_mgmt)
        .merge(ws_routes)
        .with_state(state)
}
