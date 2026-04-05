-- 0004_rls.sql: Row Level Security policies for all workspace-scoped tables
--
-- Tables WITHOUT RLS (global): users, workspaces, workspace_members, system_config, pending_embeddings
-- Tables with ws_or_global policy: model_configs, model_routing_rules

-- ============================================================================
-- acl_policies
-- ============================================================================
ALTER TABLE acl_policies ENABLE ROW LEVEL SECURITY;
ALTER TABLE acl_policies FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON acl_policies
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON acl_policies
    USING (current_setting('app.system_bypass', true) = 'true');

-- ============================================================================
-- agent_events
-- ============================================================================
ALTER TABLE agent_events ENABLE ROW LEVEL SECURITY;
ALTER TABLE agent_events FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON agent_events
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON agent_events
    USING (current_setting('app.system_bypass', true) = 'true');

-- ============================================================================
-- agent_sessions
-- ============================================================================
ALTER TABLE agent_sessions ENABLE ROW LEVEL SECURITY;
ALTER TABLE agent_sessions FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON agent_sessions
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON agent_sessions
    USING (current_setting('app.system_bypass', true) = 'true');

-- ============================================================================
-- analysis_recipes
-- ============================================================================
ALTER TABLE analysis_recipes ENABLE ROW LEVEL SECURITY;
ALTER TABLE analysis_recipes FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON analysis_recipes
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON analysis_recipes
    USING (current_setting('app.system_bypass', true) = 'true');

-- ============================================================================
-- analysis_results
-- ============================================================================
ALTER TABLE analysis_results ENABLE ROW LEVEL SECURITY;
ALTER TABLE analysis_results FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON analysis_results
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON analysis_results
    USING (current_setting('app.system_bypass', true) = 'true');

-- ============================================================================
-- approval_requests
-- ============================================================================
ALTER TABLE approval_requests ENABLE ROW LEVEL SECURITY;
ALTER TABLE approval_requests FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON approval_requests
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON approval_requests
    USING (current_setting('app.system_bypass', true) = 'true');

-- ============================================================================
-- audit_log
-- ============================================================================
ALTER TABLE audit_log ENABLE ROW LEVEL SECURITY;
ALTER TABLE audit_log FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON audit_log
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON audit_log
    USING (current_setting('app.system_bypass', true) = 'true');

-- ============================================================================
-- dashboard_widgets
-- ============================================================================
ALTER TABLE dashboard_widgets ENABLE ROW LEVEL SECURITY;
ALTER TABLE dashboard_widgets FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON dashboard_widgets
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON dashboard_widgets
    USING (current_setting('app.system_bypass', true) = 'true');

-- ============================================================================
-- dashboards
-- ============================================================================
ALTER TABLE dashboards ENABLE ROW LEVEL SECURITY;
ALTER TABLE dashboards FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON dashboards
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON dashboards
    USING (current_setting('app.system_bypass', true) = 'true');

-- ============================================================================
-- data_lineage
-- ============================================================================
ALTER TABLE data_lineage ENABLE ROW LEVEL SECURITY;
ALTER TABLE data_lineage FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON data_lineage
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON data_lineage
    USING (current_setting('app.system_bypass', true) = 'true');

-- ============================================================================
-- design_projects
-- ============================================================================
ALTER TABLE design_projects ENABLE ROW LEVEL SECURITY;
ALTER TABLE design_projects FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON design_projects
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON design_projects
    USING (current_setting('app.system_bypass', true) = 'true');

-- ============================================================================
-- knowledge_entries
-- ============================================================================
ALTER TABLE knowledge_entries ENABLE ROW LEVEL SECURITY;
ALTER TABLE knowledge_entries FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON knowledge_entries
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON knowledge_entries
    USING (current_setting('app.system_bypass', true) = 'true');

-- ============================================================================
-- load_checkpoints
-- ============================================================================
ALTER TABLE load_checkpoints ENABLE ROW LEVEL SECURITY;
ALTER TABLE load_checkpoints FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON load_checkpoints
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON load_checkpoints
    USING (current_setting('app.system_bypass', true) = 'true');

-- ============================================================================
-- memory_entries
-- ============================================================================
ALTER TABLE memory_entries ENABLE ROW LEVEL SECURITY;
ALTER TABLE memory_entries FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON memory_entries
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON memory_entries
    USING (current_setting('app.system_bypass', true) = 'true');

-- ============================================================================
-- model_configs (ws_or_global -- workspace_id can be NULL for global configs)
-- ============================================================================
ALTER TABLE model_configs ENABLE ROW LEVEL SECURITY;
ALTER TABLE model_configs FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_or_global ON model_configs
    USING (workspace_id IS NULL OR workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON model_configs
    USING (current_setting('app.system_bypass', true) = 'true');

-- ============================================================================
-- model_routing_rules (ws_or_global -- workspace_id can be NULL for global rules)
-- ============================================================================
ALTER TABLE model_routing_rules ENABLE ROW LEVEL SECURITY;
ALTER TABLE model_routing_rules FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_or_global ON model_routing_rules
    USING (workspace_id IS NULL OR workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON model_routing_rules
    USING (current_setting('app.system_bypass', true) = 'true');

-- ============================================================================
-- notification_channels
-- ============================================================================
ALTER TABLE notification_channels ENABLE ROW LEVEL SECURITY;
ALTER TABLE notification_channels FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON notification_channels
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON notification_channels
    USING (current_setting('app.system_bypass', true) = 'true');

-- ============================================================================
-- notification_log
-- ============================================================================
ALTER TABLE notification_log ENABLE ROW LEVEL SECURITY;
ALTER TABLE notification_log FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON notification_log
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON notification_log
    USING (current_setting('app.system_bypass', true) = 'true');

-- ============================================================================
-- ontology_snapshots
-- ============================================================================
ALTER TABLE ontology_snapshots ENABLE ROW LEVEL SECURITY;
ALTER TABLE ontology_snapshots FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON ontology_snapshots
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON ontology_snapshots
    USING (current_setting('app.system_bypass', true) = 'true');

-- ============================================================================
-- ontology_verifications
-- ============================================================================
ALTER TABLE ontology_verifications ENABLE ROW LEVEL SECURITY;
ALTER TABLE ontology_verifications FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON ontology_verifications
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON ontology_verifications
    USING (current_setting('app.system_bypass', true) = 'true');

-- ============================================================================
-- pinboard_items
-- ============================================================================
ALTER TABLE pinboard_items ENABLE ROW LEVEL SECURITY;
ALTER TABLE pinboard_items FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON pinboard_items
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON pinboard_items
    USING (current_setting('app.system_bypass', true) = 'true');

-- ============================================================================
-- prompt_templates
-- ============================================================================
ALTER TABLE prompt_templates ENABLE ROW LEVEL SECURITY;
ALTER TABLE prompt_templates FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_or_global ON prompt_templates
    USING (workspace_id IS NULL OR workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON prompt_templates
    USING (current_setting('app.system_bypass', true) = 'true');

-- ============================================================================
-- quality_results
-- ============================================================================
ALTER TABLE quality_results ENABLE ROW LEVEL SECURITY;
ALTER TABLE quality_results FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON quality_results
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON quality_results
    USING (current_setting('app.system_bypass', true) = 'true');

-- ============================================================================
-- quality_rules
-- ============================================================================
ALTER TABLE quality_rules ENABLE ROW LEVEL SECURITY;
ALTER TABLE quality_rules FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON quality_rules
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON quality_rules
    USING (current_setting('app.system_bypass', true) = 'true');

-- ============================================================================
-- query_executions
-- ============================================================================
ALTER TABLE query_executions ENABLE ROW LEVEL SECURITY;
ALTER TABLE query_executions FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON query_executions
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON query_executions
    USING (current_setting('app.system_bypass', true) = 'true');

-- ============================================================================
-- saved_ontologies
-- ============================================================================
ALTER TABLE saved_ontologies ENABLE ROW LEVEL SECURITY;
ALTER TABLE saved_ontologies FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON saved_ontologies
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON saved_ontologies
    USING (current_setting('app.system_bypass', true) = 'true');

-- ============================================================================
-- saved_reports
-- ============================================================================
ALTER TABLE saved_reports ENABLE ROW LEVEL SECURITY;
ALTER TABLE saved_reports FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON saved_reports
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON saved_reports
    USING (current_setting('app.system_bypass', true) = 'true');

-- ============================================================================
-- scheduled_tasks
-- ============================================================================
ALTER TABLE scheduled_tasks ENABLE ROW LEVEL SECURITY;
ALTER TABLE scheduled_tasks FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON scheduled_tasks
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON scheduled_tasks
    USING (current_setting('app.system_bypass', true) = 'true');

-- ============================================================================
-- tool_approvals
-- ============================================================================
ALTER TABLE tool_approvals ENABLE ROW LEVEL SECURITY;
ALTER TABLE tool_approvals FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON tool_approvals
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON tool_approvals
    USING (current_setting('app.system_bypass', true) = 'true');

-- ============================================================================
-- usage_records
-- ============================================================================
ALTER TABLE usage_records ENABLE ROW LEVEL SECURITY;
ALTER TABLE usage_records FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON usage_records
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON usage_records
    USING (current_setting('app.system_bypass', true) = 'true');

-- ============================================================================
-- workbench_perspectives
-- ============================================================================
ALTER TABLE workbench_perspectives ENABLE ROW LEVEL SECURITY;
ALTER TABLE workbench_perspectives FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON workbench_perspectives
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON workbench_perspectives
    USING (current_setting('app.system_bypass', true) = 'true');
