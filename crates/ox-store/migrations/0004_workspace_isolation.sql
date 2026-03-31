-- ==========================================================================
-- Migration 0004: Workspace Isolation (RLS)
--
-- Design principles:
-- 1. PostgreSQL RLS — Store trait changes = 0. Middleware sets session var.
-- 2. FORCE ROW LEVEL SECURITY — applies even to table owner.
-- 3. Column DEFAULT from session var — INSERT queries auto-fill workspace_id.
-- 4. System bypass policy — scheduled tasks and cleanup bypass RLS.
-- 5. Default workspace auto-created + backfill for existing data.
-- 6. 13 tables get workspace_id; child tables inherit via FK.
-- ==========================================================================

-- --------------------------------------------------------------------------
-- 1. Core workspace tables
-- --------------------------------------------------------------------------

CREATE TABLE workspaces (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name        VARCHAR(255) NOT NULL,
    slug        VARCHAR(100) NOT NULL UNIQUE,
    owner_id    UUID NOT NULL REFERENCES users(id),
    settings    JSONB NOT NULL DEFAULT '{}',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE workspace_members (
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    user_id      UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role         VARCHAR(20) NOT NULL DEFAULT 'member',
    joined_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (workspace_id, user_id),
    CONSTRAINT valid_workspace_role CHECK (role IN ('owner', 'admin', 'member', 'viewer'))
);

CREATE INDEX idx_workspace_members_user ON workspace_members(user_id);

-- --------------------------------------------------------------------------
-- 2. Add workspace_id to 13 existing tables (nullable for backfill phase)
-- --------------------------------------------------------------------------

ALTER TABLE design_projects        ADD COLUMN workspace_id UUID REFERENCES workspaces(id);
ALTER TABLE saved_ontologies       ADD COLUMN workspace_id UUID REFERENCES workspaces(id);
ALTER TABLE query_executions       ADD COLUMN workspace_id UUID REFERENCES workspaces(id);
ALTER TABLE dashboards             ADD COLUMN workspace_id UUID REFERENCES workspaces(id);
ALTER TABLE analysis_recipes       ADD COLUMN workspace_id UUID REFERENCES workspaces(id);
ALTER TABLE saved_reports          ADD COLUMN workspace_id UUID REFERENCES workspaces(id);
ALTER TABLE scheduled_tasks        ADD COLUMN workspace_id UUID REFERENCES workspaces(id);
ALTER TABLE agent_sessions         ADD COLUMN workspace_id UUID REFERENCES workspaces(id);
ALTER TABLE analysis_results       ADD COLUMN workspace_id UUID REFERENCES workspaces(id);
ALTER TABLE ontology_verifications ADD COLUMN workspace_id UUID REFERENCES workspaces(id);
ALTER TABLE pinboard_items         ADD COLUMN workspace_id UUID REFERENCES workspaces(id);
ALTER TABLE prompt_templates       ADD COLUMN workspace_id UUID REFERENCES workspaces(id);
ALTER TABLE memory_entries         ADD COLUMN workspace_id UUID;

-- --------------------------------------------------------------------------
-- 3. Default workspace + backfill
-- --------------------------------------------------------------------------

DO $$
DECLARE
    ws_id       UUID;
    owner_id    UUID;
    has_data    BOOLEAN;
BEGIN
    -- Check if ANY table has existing data that needs backfilling
    SELECT EXISTS(
        SELECT 1 FROM design_projects LIMIT 1
    ) OR EXISTS(
        SELECT 1 FROM saved_ontologies LIMIT 1
    ) OR EXISTS(
        SELECT 1 FROM query_executions LIMIT 1
    ) OR EXISTS(
        SELECT 1 FROM dashboards LIMIT 1
    ) OR EXISTS(
        SELECT 1 FROM memory_entries LIMIT 1
    ) INTO has_data;

    -- Find the first admin user, or any user, as workspace owner
    SELECT id INTO owner_id FROM users WHERE role = 'admin' ORDER BY created_at LIMIT 1;
    IF owner_id IS NULL THEN
        SELECT id INTO owner_id FROM users ORDER BY created_at LIMIT 1;
    END IF;

    -- If there's data but no users, create a system user to own the workspace
    IF has_data AND owner_id IS NULL THEN
        owner_id := gen_random_uuid();
        INSERT INTO users (id, email, name, provider, provider_sub, role)
        VALUES (owner_id, 'system@ontosyx.local', 'System', 'system', 'system', 'admin');
    END IF;

    -- Create default workspace and backfill if there's an owner (users exist or were just created)
    IF owner_id IS NOT NULL THEN
        ws_id := gen_random_uuid();

        INSERT INTO workspaces (id, name, slug, owner_id)
        VALUES (ws_id, 'Default', 'default', owner_id);

        INSERT INTO workspace_members (workspace_id, user_id, role)
        VALUES (ws_id, owner_id, 'owner');

        -- Add all other existing users as members
        INSERT INTO workspace_members (workspace_id, user_id, role)
        SELECT ws_id, id, 'member'
        FROM users
        WHERE id != owner_id;

        -- Backfill workspace_id on all 13 tables
        UPDATE design_projects        SET workspace_id = ws_id WHERE workspace_id IS NULL;
        UPDATE saved_ontologies       SET workspace_id = ws_id WHERE workspace_id IS NULL;
        UPDATE query_executions       SET workspace_id = ws_id WHERE workspace_id IS NULL;
        UPDATE dashboards             SET workspace_id = ws_id WHERE workspace_id IS NULL;
        UPDATE analysis_recipes       SET workspace_id = ws_id WHERE workspace_id IS NULL;
        UPDATE saved_reports          SET workspace_id = ws_id WHERE workspace_id IS NULL;
        UPDATE scheduled_tasks        SET workspace_id = ws_id WHERE workspace_id IS NULL;
        UPDATE agent_sessions         SET workspace_id = ws_id WHERE workspace_id IS NULL;
        UPDATE analysis_results       SET workspace_id = ws_id WHERE workspace_id IS NULL;
        UPDATE ontology_verifications SET workspace_id = ws_id WHERE workspace_id IS NULL;
        UPDATE pinboard_items         SET workspace_id = ws_id WHERE workspace_id IS NULL;
        UPDATE prompt_templates       SET workspace_id = ws_id WHERE workspace_id IS NULL;
        UPDATE memory_entries         SET workspace_id = ws_id  WHERE workspace_id IS NULL;
    END IF;
END $$;

-- --------------------------------------------------------------------------
-- 4. Set NOT NULL + DEFAULT from session variable
-- --------------------------------------------------------------------------
-- DEFAULT = current_setting('app.workspace_id', true)::uuid
-- This means existing INSERT queries that omit workspace_id automatically
-- get the value from the session variable set by middleware. Zero code changes.
-- --------------------------------------------------------------------------

ALTER TABLE design_projects        ALTER COLUMN workspace_id SET NOT NULL,
                                   ALTER COLUMN workspace_id SET DEFAULT current_setting('app.workspace_id', true)::uuid;
ALTER TABLE saved_ontologies       ALTER COLUMN workspace_id SET NOT NULL,
                                   ALTER COLUMN workspace_id SET DEFAULT current_setting('app.workspace_id', true)::uuid;
ALTER TABLE query_executions       ALTER COLUMN workspace_id SET NOT NULL,
                                   ALTER COLUMN workspace_id SET DEFAULT current_setting('app.workspace_id', true)::uuid;
ALTER TABLE dashboards             ALTER COLUMN workspace_id SET NOT NULL,
                                   ALTER COLUMN workspace_id SET DEFAULT current_setting('app.workspace_id', true)::uuid;
ALTER TABLE analysis_recipes       ALTER COLUMN workspace_id SET NOT NULL,
                                   ALTER COLUMN workspace_id SET DEFAULT current_setting('app.workspace_id', true)::uuid;
ALTER TABLE saved_reports          ALTER COLUMN workspace_id SET NOT NULL,
                                   ALTER COLUMN workspace_id SET DEFAULT current_setting('app.workspace_id', true)::uuid;
ALTER TABLE scheduled_tasks        ALTER COLUMN workspace_id SET NOT NULL,
                                   ALTER COLUMN workspace_id SET DEFAULT current_setting('app.workspace_id', true)::uuid;
ALTER TABLE agent_sessions         ALTER COLUMN workspace_id SET NOT NULL,
                                   ALTER COLUMN workspace_id SET DEFAULT current_setting('app.workspace_id', true)::uuid;
ALTER TABLE analysis_results       ALTER COLUMN workspace_id SET NOT NULL,
                                   ALTER COLUMN workspace_id SET DEFAULT current_setting('app.workspace_id', true)::uuid;
ALTER TABLE ontology_verifications ALTER COLUMN workspace_id SET NOT NULL,
                                   ALTER COLUMN workspace_id SET DEFAULT current_setting('app.workspace_id', true)::uuid;
ALTER TABLE pinboard_items         ALTER COLUMN workspace_id SET NOT NULL,
                                   ALTER COLUMN workspace_id SET DEFAULT current_setting('app.workspace_id', true)::uuid;
ALTER TABLE prompt_templates       ALTER COLUMN workspace_id SET NOT NULL,
                                   ALTER COLUMN workspace_id SET DEFAULT current_setting('app.workspace_id', true)::uuid;
ALTER TABLE memory_entries         ALTER COLUMN workspace_id SET NOT NULL,
                                   ALTER COLUMN workspace_id SET DEFAULT current_setting('app.workspace_id', true)::uuid;

-- FK for memory_entries (id is VARCHAR, so UUID ref needs explicit constraint)
ALTER TABLE memory_entries
    ADD CONSTRAINT fk_memory_workspace FOREIGN KEY (workspace_id) REFERENCES workspaces(id);

-- --------------------------------------------------------------------------
-- 5. Row-Level Security policies
-- --------------------------------------------------------------------------
-- Two permissive policies per table (OR semantics):
-- 1. ws_isolation: workspace_id matches session variable
-- 2. system_bypass: allows system operations (scheduled tasks, cleanup)
--
-- When neither app.workspace_id nor app.system_bypass is set, no rows
-- are visible — safe deny-all default.
-- --------------------------------------------------------------------------

DO $$
DECLARE
    tbl TEXT;
BEGIN
    FOREACH tbl IN ARRAY ARRAY[
        'design_projects', 'saved_ontologies', 'query_executions',
        'dashboards', 'analysis_recipes', 'saved_reports',
        'scheduled_tasks', 'agent_sessions', 'analysis_results',
        'ontology_verifications', 'pinboard_items', 'prompt_templates',
        'memory_entries'
    ] LOOP
        EXECUTE format('ALTER TABLE %I ENABLE ROW LEVEL SECURITY', tbl);
        EXECUTE format('ALTER TABLE %I FORCE ROW LEVEL SECURITY', tbl);

        -- Policy 1: workspace isolation (normal requests)
        EXECUTE format('DROP POLICY IF EXISTS ws_isolation ON %I', tbl);
        EXECUTE format(
            'CREATE POLICY ws_isolation ON %I
                USING (workspace_id = current_setting(''app.workspace_id'', true)::uuid)
                WITH CHECK (workspace_id = current_setting(''app.workspace_id'', true)::uuid)',
            tbl
        );

        -- Policy 2: system bypass (scheduled tasks, cleanup, migrations)
        EXECUTE format('DROP POLICY IF EXISTS system_bypass ON %I', tbl);
        EXECUTE format(
            'CREATE POLICY system_bypass ON %I
                USING (current_setting(''app.system_bypass'', true) = ''true'')',
            tbl
        );
    END LOOP;
END $$;

-- --------------------------------------------------------------------------
-- 6. Composite indices for workspace-scoped queries
-- --------------------------------------------------------------------------

CREATE INDEX idx_projects_workspace       ON design_projects(workspace_id, created_at DESC);
CREATE INDEX idx_ontologies_workspace     ON saved_ontologies(workspace_id, created_at DESC);
CREATE INDEX idx_queries_workspace        ON query_executions(workspace_id, created_at DESC);
CREATE INDEX idx_dashboards_workspace     ON dashboards(workspace_id, updated_at DESC);
CREATE INDEX idx_recipes_workspace        ON analysis_recipes(workspace_id, created_at DESC);
CREATE INDEX idx_reports_workspace        ON saved_reports(workspace_id, updated_at DESC);
CREATE INDEX idx_schedtasks_workspace     ON scheduled_tasks(workspace_id);
CREATE INDEX idx_sessions_workspace       ON agent_sessions(workspace_id, created_at DESC);
CREATE INDEX idx_results_workspace        ON analysis_results(workspace_id, created_at DESC);
CREATE INDEX idx_verifications_workspace  ON ontology_verifications(workspace_id);
CREATE INDEX idx_pins_workspace           ON pinboard_items(workspace_id);
CREATE INDEX idx_templates_workspace      ON prompt_templates(workspace_id);
CREATE INDEX idx_memory_workspace         ON memory_entries(workspace_id);
