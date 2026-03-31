-- ============================================================
-- 0008: Approval Workflows — configurable gates for schema
--       deployment, migration, and destructive actions
-- ============================================================
-- Workspace admins can review and approve/reject pending actions
-- before they execute. Workspace-scoped via RLS.
-- ============================================================

CREATE TABLE IF NOT EXISTS approval_requests (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id    UUID NOT NULL REFERENCES workspaces(id),
    requester_id    UUID NOT NULL REFERENCES users(id),
    action_type     TEXT NOT NULL,    -- 'deploy_schema', 'migrate_schema', 'delete_project'
    resource_type   TEXT NOT NULL,    -- 'project', 'ontology'
    resource_id     TEXT NOT NULL,
    payload         JSONB NOT NULL DEFAULT '{}',  -- serialized action parameters
    status          TEXT NOT NULL DEFAULT 'pending'
                    CHECK (status IN ('pending', 'approved', 'rejected', 'expired')),
    reviewer_id     UUID REFERENCES users(id),
    review_notes    TEXT,
    reviewed_at     TIMESTAMPTZ,
    expires_at      TIMESTAMPTZ NOT NULL DEFAULT (NOW() + INTERVAL '7 days'),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Indexes for common access patterns
CREATE INDEX IF NOT EXISTS idx_approval_workspace_status
    ON approval_requests(workspace_id, status, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_approval_requester
    ON approval_requests(requester_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_approval_resource
    ON approval_requests(resource_type, resource_id);
CREATE INDEX IF NOT EXISTS idx_approval_expires
    ON approval_requests(expires_at) WHERE status = 'pending';

-- RLS policies (same pattern as 0005)
ALTER TABLE approval_requests ENABLE ROW LEVEL SECURITY;
ALTER TABLE approval_requests FORCE ROW LEVEL SECURITY;

CREATE POLICY ws_isolation ON approval_requests
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid);

CREATE POLICY system_bypass ON approval_requests
    USING (current_setting('app.system_bypass', true) = 'true');

-- Default workspace_id from session variable (auto-set on INSERT)
ALTER TABLE approval_requests
    ALTER COLUMN workspace_id
    SET DEFAULT current_setting('app.workspace_id', true)::uuid;
