-- 0003_foreign_keys.sql: All foreign key constraints (consolidated from pg_dump)

-- ============================================================================
-- workspaces
-- ============================================================================
ALTER TABLE ONLY workspaces
    ADD CONSTRAINT workspaces_owner_id_fkey FOREIGN KEY (owner_id) REFERENCES users(id);

-- ============================================================================
-- workspace_members
-- ============================================================================
ALTER TABLE ONLY workspace_members
    ADD CONSTRAINT workspace_members_user_id_fkey FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE;
ALTER TABLE ONLY workspace_members
    ADD CONSTRAINT workspace_members_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;

-- ============================================================================
-- design_projects
-- ============================================================================
ALTER TABLE ONLY design_projects
    ADD CONSTRAINT design_projects_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;
ALTER TABLE ONLY design_projects
    ADD CONSTRAINT design_projects_saved_ontology_ws_fk FOREIGN KEY (workspace_id, saved_ontology_id) REFERENCES saved_ontologies(workspace_id, id) ON DELETE SET NULL;

-- ============================================================================
-- ontology_snapshots
-- ============================================================================
ALTER TABLE ONLY ontology_snapshots
    ADD CONSTRAINT ontology_snapshots_workspace_fk FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;
ALTER TABLE ONLY ontology_snapshots
    ADD CONSTRAINT ontology_snapshots_project_ws_fk FOREIGN KEY (workspace_id, project_id) REFERENCES design_projects(workspace_id, id) ON DELETE CASCADE;

-- ============================================================================
-- saved_ontologies
-- ============================================================================
ALTER TABLE ONLY saved_ontologies
    ADD CONSTRAINT saved_ontologies_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;

-- ============================================================================
-- query_executions
-- ============================================================================
ALTER TABLE ONLY query_executions
    ADD CONSTRAINT query_executions_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;
ALTER TABLE ONLY query_executions
    ADD CONSTRAINT query_executions_saved_ontology_ws_fk FOREIGN KEY (workspace_id, saved_ontology_id) REFERENCES saved_ontologies(workspace_id, id) ON DELETE RESTRICT;

-- ============================================================================
-- pinboard_items
-- ============================================================================
ALTER TABLE ONLY pinboard_items
    ADD CONSTRAINT pinboard_items_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;
ALTER TABLE ONLY pinboard_items
    ADD CONSTRAINT pinboard_items_query_execution_ws_fk FOREIGN KEY (workspace_id, query_execution_id) REFERENCES query_executions(workspace_id, id) ON DELETE CASCADE;

-- ============================================================================
-- agent_sessions
-- ============================================================================
ALTER TABLE ONLY agent_sessions
    ADD CONSTRAINT agent_sessions_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;

-- ============================================================================
-- agent_events
-- ============================================================================
ALTER TABLE ONLY agent_events
    ADD CONSTRAINT agent_events_workspace_fk FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;
ALTER TABLE ONLY agent_events
    ADD CONSTRAINT agent_events_session_ws_fk FOREIGN KEY (workspace_id, session_id) REFERENCES agent_sessions(workspace_id, id) ON DELETE CASCADE;

-- ============================================================================
-- tool_approvals
-- ============================================================================
ALTER TABLE ONLY tool_approvals
    ADD CONSTRAINT tool_approvals_workspace_fk FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;
ALTER TABLE ONLY tool_approvals
    ADD CONSTRAINT tool_approvals_session_ws_fk FOREIGN KEY (workspace_id, session_id) REFERENCES agent_sessions(workspace_id, id) ON DELETE CASCADE;

-- ============================================================================
-- dashboards
-- ============================================================================
ALTER TABLE ONLY dashboards
    ADD CONSTRAINT dashboards_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;

-- ============================================================================
-- dashboard_widgets
-- ============================================================================
ALTER TABLE ONLY dashboard_widgets
    ADD CONSTRAINT dashboard_widgets_workspace_fk FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;
ALTER TABLE ONLY dashboard_widgets
    ADD CONSTRAINT dashboard_widgets_dashboard_ws_fk FOREIGN KEY (workspace_id, dashboard_id) REFERENCES dashboards(workspace_id, id) ON DELETE CASCADE;

-- ============================================================================
-- data_lineage
-- ============================================================================
ALTER TABLE ONLY data_lineage
    ADD CONSTRAINT data_lineage_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;
ALTER TABLE ONLY data_lineage
    ADD CONSTRAINT data_lineage_project_ws_fk FOREIGN KEY (workspace_id, project_id) REFERENCES design_projects(workspace_id, id) ON DELETE SET NULL;
ALTER TABLE ONLY data_lineage
    ADD CONSTRAINT data_lineage_loaded_by_fkey FOREIGN KEY (loaded_by) REFERENCES users(id);

-- ============================================================================
-- analysis_recipes
-- ============================================================================
ALTER TABLE ONLY analysis_recipes
    ADD CONSTRAINT analysis_recipes_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;
ALTER TABLE ONLY analysis_recipes
    ADD CONSTRAINT analysis_recipes_parent_id_fkey FOREIGN KEY (parent_id) REFERENCES analysis_recipes(id) ON DELETE SET NULL;

-- ============================================================================
-- analysis_results
-- ============================================================================
ALTER TABLE ONLY analysis_results
    ADD CONSTRAINT analysis_results_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;
ALTER TABLE ONLY analysis_results
    ADD CONSTRAINT analysis_results_recipe_ws_fk FOREIGN KEY (workspace_id, recipe_id) REFERENCES analysis_recipes(workspace_id, id) ON DELETE CASCADE;

-- ============================================================================
-- scheduled_tasks
-- ============================================================================
ALTER TABLE ONLY scheduled_tasks
    ADD CONSTRAINT scheduled_tasks_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;
ALTER TABLE ONLY scheduled_tasks
    ADD CONSTRAINT scheduled_tasks_recipe_ws_fk FOREIGN KEY (workspace_id, recipe_id) REFERENCES analysis_recipes(workspace_id, id) ON DELETE CASCADE;

-- ============================================================================
-- quality_rules
-- ============================================================================
ALTER TABLE ONLY quality_rules
    ADD CONSTRAINT quality_rules_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;
ALTER TABLE ONLY quality_rules
    ADD CONSTRAINT quality_rules_created_by_fkey FOREIGN KEY (created_by) REFERENCES users(id);

-- ============================================================================
-- quality_results
-- ============================================================================
ALTER TABLE ONLY quality_results
    ADD CONSTRAINT quality_results_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;
ALTER TABLE ONLY quality_results
    ADD CONSTRAINT quality_results_rule_ws_fk FOREIGN KEY (workspace_id, rule_id) REFERENCES quality_rules(workspace_id, id) ON DELETE CASCADE;

-- ============================================================================
-- knowledge_entries
-- ============================================================================
ALTER TABLE ONLY knowledge_entries
    ADD CONSTRAINT knowledge_entries_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;
ALTER TABLE ONLY knowledge_entries
    ADD CONSTRAINT knowledge_entries_reviewed_by_fkey FOREIGN KEY (reviewed_by) REFERENCES users(id) ON DELETE SET NULL;

-- ============================================================================
-- memory_entries
-- ============================================================================
ALTER TABLE ONLY memory_entries
    ADD CONSTRAINT fk_memory_workspace FOREIGN KEY (workspace_id) REFERENCES workspaces(id);
ALTER TABLE ONLY memory_entries
    ADD CONSTRAINT memory_entries_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;

-- ============================================================================
-- model_configs
-- ============================================================================
ALTER TABLE ONLY model_configs
    ADD CONSTRAINT model_configs_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;

-- ============================================================================
-- model_routing_rules
-- ============================================================================
ALTER TABLE ONLY model_routing_rules
    ADD CONSTRAINT model_routing_rules_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;
ALTER TABLE ONLY model_routing_rules
    ADD CONSTRAINT model_routing_rules_model_config_id_fkey FOREIGN KEY (model_config_id) REFERENCES model_configs(id) ON DELETE CASCADE;

-- ============================================================================
-- notification_log
-- ============================================================================
ALTER TABLE ONLY notification_log
    ADD CONSTRAINT notification_log_channel_id_fkey FOREIGN KEY (channel_id) REFERENCES notification_channels(id) ON DELETE CASCADE;

-- ============================================================================
-- prompt_templates
-- ============================================================================
ALTER TABLE ONLY prompt_templates
    ADD CONSTRAINT prompt_templates_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;

-- ============================================================================
-- acl_policies
-- ============================================================================
ALTER TABLE ONLY acl_policies
    ADD CONSTRAINT acl_policies_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;
ALTER TABLE ONLY acl_policies
    ADD CONSTRAINT acl_policies_created_by_fkey FOREIGN KEY (created_by) REFERENCES users(id);

-- ============================================================================
-- approval_requests
-- ============================================================================
ALTER TABLE ONLY approval_requests
    ADD CONSTRAINT approval_requests_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;
ALTER TABLE ONLY approval_requests
    ADD CONSTRAINT approval_requests_requester_id_fkey FOREIGN KEY (requester_id) REFERENCES users(id);
ALTER TABLE ONLY approval_requests
    ADD CONSTRAINT approval_requests_reviewer_id_fkey FOREIGN KEY (reviewer_id) REFERENCES users(id);

-- ============================================================================
-- audit_log
-- ============================================================================
ALTER TABLE ONLY audit_log
    ADD CONSTRAINT audit_log_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;
ALTER TABLE ONLY audit_log
    ADD CONSTRAINT audit_log_user_id_fkey FOREIGN KEY (user_id) REFERENCES users(id);

-- ============================================================================
-- usage_records
-- ============================================================================
ALTER TABLE ONLY usage_records
    ADD CONSTRAINT usage_records_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;
ALTER TABLE ONLY usage_records
    ADD CONSTRAINT usage_records_user_id_fkey FOREIGN KEY (user_id) REFERENCES users(id);

-- ============================================================================
-- ontology_verifications
-- ============================================================================
ALTER TABLE ONLY ontology_verifications
    ADD CONSTRAINT ontology_verifications_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;
ALTER TABLE ONLY ontology_verifications
    ADD CONSTRAINT ontology_verifications_verified_by_fkey FOREIGN KEY (verified_by) REFERENCES users(id) ON DELETE CASCADE;

-- ============================================================================
-- saved_reports
-- ============================================================================
ALTER TABLE ONLY saved_reports
    ADD CONSTRAINT saved_reports_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;

-- ============================================================================
-- workbench_perspectives
-- ============================================================================
ALTER TABLE ONLY workbench_perspectives
    ADD CONSTRAINT workbench_perspectives_workspace_fk FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;
ALTER TABLE ONLY workbench_perspectives
    ADD CONSTRAINT workbench_perspectives_project_ws_fk FOREIGN KEY (workspace_id, project_id) REFERENCES design_projects(workspace_id, id) ON DELETE SET NULL;
