-- ============================================================
-- 0011: Cascade Deletes — data integrity on workspace/project removal
-- ============================================================
-- CRITICAL FIX: workspace_id FKs lacked ON DELETE CASCADE.
-- Deleting a workspace left orphaned records in 20+ tables.
-- Also fixes project_id cascade for lineage entries.
--
-- PostgreSQL ALTER TABLE ... DROP/ADD CONSTRAINT pattern:
--   1. Drop the existing FK constraint
--   2. Re-add with ON DELETE CASCADE
-- ============================================================

-- Helper: idempotent constraint replacement
-- (constraint names come from PostgreSQL's auto-naming: tablename_columnname_fkey)

-- === Original 13 tables from migration 0004 ===

ALTER TABLE design_projects
    DROP CONSTRAINT IF EXISTS design_projects_workspace_id_fkey,
    ADD CONSTRAINT design_projects_workspace_id_fkey
        FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;

ALTER TABLE saved_ontologies
    DROP CONSTRAINT IF EXISTS saved_ontologies_workspace_id_fkey,
    ADD CONSTRAINT saved_ontologies_workspace_id_fkey
        FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;

ALTER TABLE query_executions
    DROP CONSTRAINT IF EXISTS query_executions_workspace_id_fkey,
    ADD CONSTRAINT query_executions_workspace_id_fkey
        FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;

ALTER TABLE dashboards
    DROP CONSTRAINT IF EXISTS dashboards_workspace_id_fkey,
    ADD CONSTRAINT dashboards_workspace_id_fkey
        FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;

ALTER TABLE analysis_recipes
    DROP CONSTRAINT IF EXISTS analysis_recipes_workspace_id_fkey,
    ADD CONSTRAINT analysis_recipes_workspace_id_fkey
        FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;

ALTER TABLE saved_reports
    DROP CONSTRAINT IF EXISTS saved_reports_workspace_id_fkey,
    ADD CONSTRAINT saved_reports_workspace_id_fkey
        FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;

ALTER TABLE scheduled_tasks
    DROP CONSTRAINT IF EXISTS scheduled_tasks_workspace_id_fkey,
    ADD CONSTRAINT scheduled_tasks_workspace_id_fkey
        FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;

ALTER TABLE agent_sessions
    DROP CONSTRAINT IF EXISTS agent_sessions_workspace_id_fkey,
    ADD CONSTRAINT agent_sessions_workspace_id_fkey
        FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;

ALTER TABLE analysis_results
    DROP CONSTRAINT IF EXISTS analysis_results_workspace_id_fkey,
    ADD CONSTRAINT analysis_results_workspace_id_fkey
        FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;

ALTER TABLE ontology_verifications
    DROP CONSTRAINT IF EXISTS ontology_verifications_workspace_id_fkey,
    ADD CONSTRAINT ontology_verifications_workspace_id_fkey
        FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;

ALTER TABLE pinboard_items
    DROP CONSTRAINT IF EXISTS pinboard_items_workspace_id_fkey,
    ADD CONSTRAINT pinboard_items_workspace_id_fkey
        FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;

ALTER TABLE prompt_templates
    DROP CONSTRAINT IF EXISTS prompt_templates_workspace_id_fkey,
    ADD CONSTRAINT prompt_templates_workspace_id_fkey
        FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;

ALTER TABLE memory_entries
    DROP CONSTRAINT IF EXISTS memory_entries_workspace_id_fkey,
    ADD CONSTRAINT memory_entries_workspace_id_fkey
        FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;

-- === New tables from migrations 0005-0010 ===

ALTER TABLE audit_log
    DROP CONSTRAINT IF EXISTS audit_log_workspace_id_fkey,
    ADD CONSTRAINT audit_log_workspace_id_fkey
        FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;

ALTER TABLE usage_records
    DROP CONSTRAINT IF EXISTS usage_records_workspace_id_fkey,
    ADD CONSTRAINT usage_records_workspace_id_fkey
        FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;

ALTER TABLE data_lineage
    DROP CONSTRAINT IF EXISTS data_lineage_workspace_id_fkey,
    ADD CONSTRAINT data_lineage_workspace_id_fkey
        FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;

-- Also cascade project deletion → lineage cleanup
ALTER TABLE data_lineage
    DROP CONSTRAINT IF EXISTS data_lineage_project_id_fkey,
    ADD CONSTRAINT data_lineage_project_id_fkey
        FOREIGN KEY (project_id) REFERENCES design_projects(id) ON DELETE SET NULL;

ALTER TABLE approval_requests
    DROP CONSTRAINT IF EXISTS approval_requests_workspace_id_fkey,
    ADD CONSTRAINT approval_requests_workspace_id_fkey
        FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;

ALTER TABLE quality_rules
    DROP CONSTRAINT IF EXISTS quality_rules_workspace_id_fkey,
    ADD CONSTRAINT quality_rules_workspace_id_fkey
        FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;

ALTER TABLE quality_results
    DROP CONSTRAINT IF EXISTS quality_results_workspace_id_fkey,
    ADD CONSTRAINT quality_results_workspace_id_fkey
        FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;

ALTER TABLE acl_policies
    DROP CONSTRAINT IF EXISTS acl_policies_workspace_id_fkey,
    ADD CONSTRAINT acl_policies_workspace_id_fkey
        FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;
