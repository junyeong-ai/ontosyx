-- ============================================================
-- 0013: Compound workspace FK enforcement
-- ============================================================
-- Problem: Cross-table FKs allow referencing rows from other
-- workspaces. RLS prevents reading them, but orphan references
-- cause silent data integrity issues (e.g., project in WS-A
-- pointing to ontology in WS-B).
--
-- Solution: Compound FKs that include workspace_id, enforced
-- at the database level. This makes it impossible to create
-- cross-workspace references regardless of application logic.
--
-- Strategy:
--   1. Add UNIQUE(workspace_id, id) to parent tables
--   2. Add workspace_id to child tables that lack it
--   3. Replace simple FKs with compound FKs
--   4. Add trigger validation for nullable/optional FKs
-- ============================================================

BEGIN;

-- Enable system bypass for this migration transaction.
-- Without this, RLS on ontosyx_app blocks the UPDATE JOINs
-- that backfill workspace_id from parent tables.
SET LOCAL app.system_bypass = 'true';

-- ============================================================
-- Phase 1: Add compound unique indices to parent tables
-- ============================================================
-- These are needed for compound FK references.
-- They don't affect existing queries — just add an index.

CREATE UNIQUE INDEX IF NOT EXISTS uq_saved_ontologies_ws_id
  ON saved_ontologies (workspace_id, id);

CREATE UNIQUE INDEX IF NOT EXISTS uq_design_projects_ws_id
  ON design_projects (workspace_id, id);

CREATE UNIQUE INDEX IF NOT EXISTS uq_dashboards_ws_id
  ON dashboards (workspace_id, id);

CREATE UNIQUE INDEX IF NOT EXISTS uq_analysis_recipes_ws_id
  ON analysis_recipes (workspace_id, id);

CREATE UNIQUE INDEX IF NOT EXISTS uq_query_executions_ws_id
  ON query_executions (workspace_id, id);

CREATE UNIQUE INDEX IF NOT EXISTS uq_agent_sessions_ws_id
  ON agent_sessions (workspace_id, id);

CREATE UNIQUE INDEX IF NOT EXISTS uq_quality_rules_ws_id
  ON quality_rules (workspace_id, id);


-- ============================================================
-- Phase 2: Add workspace_id to child tables that lack it
-- ============================================================
-- Backfill from the parent table via existing FK.

-- agent_events: workspace from agent_sessions
ALTER TABLE agent_events
  ADD COLUMN IF NOT EXISTS workspace_id UUID;

UPDATE agent_events ae
  SET workspace_id = s.workspace_id
  FROM agent_sessions s
  WHERE ae.session_id = s.id AND ae.workspace_id IS NULL;

ALTER TABLE agent_events
  ALTER COLUMN workspace_id SET NOT NULL,
  ALTER COLUMN workspace_id SET DEFAULT current_setting('app.workspace_id', true)::uuid;

-- dashboard_widgets: workspace from dashboards
ALTER TABLE dashboard_widgets
  ADD COLUMN IF NOT EXISTS workspace_id UUID;

UPDATE dashboard_widgets dw
  SET workspace_id = d.workspace_id
  FROM dashboards d
  WHERE dw.dashboard_id = d.id AND dw.workspace_id IS NULL;

ALTER TABLE dashboard_widgets
  ALTER COLUMN workspace_id SET NOT NULL,
  ALTER COLUMN workspace_id SET DEFAULT current_setting('app.workspace_id', true)::uuid;

-- ontology_snapshots: workspace from design_projects
ALTER TABLE ontology_snapshots
  ADD COLUMN IF NOT EXISTS workspace_id UUID;

UPDATE ontology_snapshots os
  SET workspace_id = dp.workspace_id
  FROM design_projects dp
  WHERE os.project_id = dp.id AND os.workspace_id IS NULL;

ALTER TABLE ontology_snapshots
  ALTER COLUMN workspace_id SET NOT NULL,
  ALTER COLUMN workspace_id SET DEFAULT current_setting('app.workspace_id', true)::uuid;

-- tool_approvals: workspace from agent_sessions
ALTER TABLE tool_approvals
  ADD COLUMN IF NOT EXISTS workspace_id UUID;

UPDATE tool_approvals ta
  SET workspace_id = s.workspace_id
  FROM agent_sessions s
  WHERE ta.session_id = s.id AND ta.workspace_id IS NULL;

ALTER TABLE tool_approvals
  ALTER COLUMN workspace_id SET NOT NULL,
  ALTER COLUMN workspace_id SET DEFAULT current_setting('app.workspace_id', true)::uuid;

-- workbench_perspectives: workspace from design_projects (nullable FK)
ALTER TABLE workbench_perspectives
  ADD COLUMN IF NOT EXISTS workspace_id UUID;

UPDATE workbench_perspectives wp
  SET workspace_id = dp.workspace_id
  FROM design_projects dp
  WHERE wp.project_id = dp.id AND wp.workspace_id IS NULL;

-- Perspectives without a project: use the first workspace as fallback
-- (migration context has no session workspace variable)
UPDATE workbench_perspectives wp
  SET workspace_id = (SELECT id FROM workspaces ORDER BY created_at LIMIT 1)
  WHERE wp.workspace_id IS NULL;

ALTER TABLE workbench_perspectives
  ALTER COLUMN workspace_id SET NOT NULL,
  ALTER COLUMN workspace_id SET DEFAULT current_setting('app.workspace_id', true)::uuid;


-- ============================================================
-- Phase 3: RLS policies for newly workspace-scoped tables
-- ============================================================

ALTER TABLE agent_events ENABLE ROW LEVEL SECURITY;
ALTER TABLE agent_events FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON agent_events
  USING (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON agent_events
  USING (current_setting('app.system_bypass', true) = 'true');

ALTER TABLE dashboard_widgets ENABLE ROW LEVEL SECURITY;
ALTER TABLE dashboard_widgets FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON dashboard_widgets
  USING (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON dashboard_widgets
  USING (current_setting('app.system_bypass', true) = 'true');

ALTER TABLE ontology_snapshots ENABLE ROW LEVEL SECURITY;
ALTER TABLE ontology_snapshots FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON ontology_snapshots
  USING (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON ontology_snapshots
  USING (current_setting('app.system_bypass', true) = 'true');

ALTER TABLE tool_approvals ENABLE ROW LEVEL SECURITY;
ALTER TABLE tool_approvals FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON tool_approvals
  USING (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON tool_approvals
  USING (current_setting('app.system_bypass', true) = 'true');

ALTER TABLE workbench_perspectives ENABLE ROW LEVEL SECURITY;
ALTER TABLE workbench_perspectives FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON workbench_perspectives
  USING (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON workbench_perspectives
  USING (current_setting('app.system_bypass', true) = 'true');


-- ============================================================
-- Phase 4: Replace simple FKs with compound FKs
-- ============================================================
-- Each FK now includes workspace_id, making cross-workspace
-- references impossible at the database level.

-- 4a. design_projects.saved_ontology_id → saved_ontologies
ALTER TABLE design_projects
  DROP CONSTRAINT design_projects_saved_ontology_id_fkey;
ALTER TABLE design_projects
  ADD CONSTRAINT design_projects_saved_ontology_ws_fk
    FOREIGN KEY (workspace_id, saved_ontology_id)
    REFERENCES saved_ontologies (workspace_id, id)
    ON DELETE SET NULL;

-- 4b. query_executions.saved_ontology_id → saved_ontologies
ALTER TABLE query_executions
  DROP CONSTRAINT query_executions_saved_ontology_id_fkey;
ALTER TABLE query_executions
  ADD CONSTRAINT query_executions_saved_ontology_ws_fk
    FOREIGN KEY (workspace_id, saved_ontology_id)
    REFERENCES saved_ontologies (workspace_id, id)
    ON DELETE RESTRICT;

-- 4c. ontology_snapshots.project_id → design_projects
ALTER TABLE ontology_snapshots
  DROP CONSTRAINT ontology_snapshots_project_id_fkey;
ALTER TABLE ontology_snapshots
  ADD CONSTRAINT ontology_snapshots_project_ws_fk
    FOREIGN KEY (workspace_id, project_id)
    REFERENCES design_projects (workspace_id, id)
    ON DELETE CASCADE;

-- 4d. dashboard_widgets.dashboard_id → dashboards
ALTER TABLE dashboard_widgets
  DROP CONSTRAINT dashboard_widgets_dashboard_id_fkey;
ALTER TABLE dashboard_widgets
  ADD CONSTRAINT dashboard_widgets_dashboard_ws_fk
    FOREIGN KEY (workspace_id, dashboard_id)
    REFERENCES dashboards (workspace_id, id)
    ON DELETE CASCADE;

-- 4e. agent_events.session_id → agent_sessions
ALTER TABLE agent_events
  DROP CONSTRAINT agent_events_session_id_fkey;
ALTER TABLE agent_events
  ADD CONSTRAINT agent_events_session_ws_fk
    FOREIGN KEY (workspace_id, session_id)
    REFERENCES agent_sessions (workspace_id, id)
    ON DELETE CASCADE;

-- 4f. tool_approvals.session_id → agent_sessions
ALTER TABLE tool_approvals
  DROP CONSTRAINT tool_approvals_session_id_fkey;
ALTER TABLE tool_approvals
  ADD CONSTRAINT tool_approvals_session_ws_fk
    FOREIGN KEY (workspace_id, session_id)
    REFERENCES agent_sessions (workspace_id, id)
    ON DELETE CASCADE;

-- 4g. analysis_results.recipe_id → analysis_recipes
ALTER TABLE analysis_results
  DROP CONSTRAINT analysis_results_recipe_id_fkey;
ALTER TABLE analysis_results
  ADD CONSTRAINT analysis_results_recipe_ws_fk
    FOREIGN KEY (workspace_id, recipe_id)
    REFERENCES analysis_recipes (workspace_id, id)
    ON DELETE CASCADE;

-- 4h. scheduled_tasks.recipe_id → analysis_recipes
ALTER TABLE scheduled_tasks
  DROP CONSTRAINT scheduled_tasks_recipe_id_fkey;
ALTER TABLE scheduled_tasks
  ADD CONSTRAINT scheduled_tasks_recipe_ws_fk
    FOREIGN KEY (workspace_id, recipe_id)
    REFERENCES analysis_recipes (workspace_id, id)
    ON DELETE CASCADE;

-- 4i. pinboard_items.query_execution_id → query_executions
ALTER TABLE pinboard_items
  DROP CONSTRAINT pinboard_items_query_execution_id_fkey;
ALTER TABLE pinboard_items
  ADD CONSTRAINT pinboard_items_query_execution_ws_fk
    FOREIGN KEY (workspace_id, query_execution_id)
    REFERENCES query_executions (workspace_id, id)
    ON DELETE CASCADE;

-- 4j. quality_results.rule_id → quality_rules
ALTER TABLE quality_results
  DROP CONSTRAINT quality_results_rule_id_fkey;
ALTER TABLE quality_results
  ADD CONSTRAINT quality_results_rule_ws_fk
    FOREIGN KEY (workspace_id, rule_id)
    REFERENCES quality_rules (workspace_id, id)
    ON DELETE CASCADE;

-- 4k. workbench_perspectives.project_id → design_projects (nullable)
ALTER TABLE workbench_perspectives
  DROP CONSTRAINT workbench_perspectives_project_id_fkey;
-- Compound FK with nullable column: only enforced when project_id IS NOT NULL
ALTER TABLE workbench_perspectives
  ADD CONSTRAINT workbench_perspectives_project_ws_fk
    FOREIGN KEY (workspace_id, project_id)
    REFERENCES design_projects (workspace_id, id)
    ON DELETE SET NULL;

-- 4l. data_lineage.project_id → design_projects (nullable, SET NULL)
ALTER TABLE data_lineage
  DROP CONSTRAINT data_lineage_project_id_fkey;
ALTER TABLE data_lineage
  ADD CONSTRAINT data_lineage_project_ws_fk
    FOREIGN KEY (workspace_id, project_id)
    REFERENCES design_projects (workspace_id, id)
    ON DELETE SET NULL;


-- ============================================================
-- Phase 5: Workspace FK for new child tables → workspaces
-- ============================================================

ALTER TABLE agent_events
  ADD CONSTRAINT agent_events_workspace_fk
    FOREIGN KEY (workspace_id) REFERENCES workspaces (id) ON DELETE CASCADE;

ALTER TABLE dashboard_widgets
  ADD CONSTRAINT dashboard_widgets_workspace_fk
    FOREIGN KEY (workspace_id) REFERENCES workspaces (id) ON DELETE CASCADE;

ALTER TABLE ontology_snapshots
  ADD CONSTRAINT ontology_snapshots_workspace_fk
    FOREIGN KEY (workspace_id) REFERENCES workspaces (id) ON DELETE CASCADE;

ALTER TABLE tool_approvals
  ADD CONSTRAINT tool_approvals_workspace_fk
    FOREIGN KEY (workspace_id) REFERENCES workspaces (id) ON DELETE CASCADE;

ALTER TABLE workbench_perspectives
  ADD CONSTRAINT workbench_perspectives_workspace_fk
    FOREIGN KEY (workspace_id) REFERENCES workspaces (id) ON DELETE CASCADE;


-- ============================================================
-- Phase 6: Performance indices for compound FK lookups
-- ============================================================

CREATE INDEX IF NOT EXISTS idx_agent_events_ws
  ON agent_events (workspace_id, session_id);

CREATE INDEX IF NOT EXISTS idx_dashboard_widgets_ws
  ON dashboard_widgets (workspace_id, dashboard_id);

CREATE INDEX IF NOT EXISTS idx_ontology_snapshots_ws
  ON ontology_snapshots (workspace_id, project_id);

CREATE INDEX IF NOT EXISTS idx_tool_approvals_ws
  ON tool_approvals (workspace_id, session_id);

CREATE INDEX IF NOT EXISTS idx_workbench_perspectives_ws
  ON workbench_perspectives (workspace_id);

COMMIT;
