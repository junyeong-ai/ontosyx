-- ============================================================
-- 0005: Audit Log — append-only event log for CRUD operations
-- ============================================================
-- Captures who did what, when, and where for enterprise governance.
-- Workspace-scoped via RLS (same pattern as all other tables).
-- ============================================================

CREATE TABLE IF NOT EXISTS audit_log (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id       UUID REFERENCES users(id),
    workspace_id  UUID NOT NULL REFERENCES workspaces(id),
    action        TEXT NOT NULL,       -- 'project.create', 'ontology.deploy', 'config.update', etc.
    resource_type TEXT NOT NULL,       -- 'project', 'ontology', 'dashboard', 'workspace', 'config'
    resource_id   TEXT,                -- UUID or identifier of the affected resource
    details       JSONB DEFAULT '{}',  -- Action-specific metadata (e.g., old/new values)
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Indexes for common access patterns
CREATE INDEX IF NOT EXISTS idx_audit_log_workspace   ON audit_log(workspace_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_audit_log_user        ON audit_log(user_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_audit_log_resource    ON audit_log(resource_type, resource_id);
CREATE INDEX IF NOT EXISTS idx_audit_log_action      ON audit_log(action, created_at DESC);

-- RLS policies (same pattern as 0004)
ALTER TABLE audit_log ENABLE ROW LEVEL SECURITY;
ALTER TABLE audit_log FORCE ROW LEVEL SECURITY;

CREATE POLICY ws_isolation ON audit_log
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid);

CREATE POLICY system_bypass ON audit_log
    USING (current_setting('app.system_bypass', true) = 'true');

-- Default workspace_id from session variable (auto-set on INSERT)
ALTER TABLE audit_log
    ALTER COLUMN workspace_id
    SET DEFAULT current_setting('app.workspace_id', true)::uuid;
