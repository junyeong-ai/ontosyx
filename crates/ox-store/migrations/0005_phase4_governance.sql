-- Phase 4 governance extensions
-- Run after 0004_rls.sql

-- 4.4 Audit log: track which workspace was affected by system maintenance.
ALTER TABLE audit_log ADD COLUMN IF NOT EXISTS affected_workspace_id UUID;
CREATE INDEX IF NOT EXISTS idx_audit_affected_ws
    ON audit_log (affected_workspace_id)
    WHERE affected_workspace_id IS NOT NULL;

-- Extend the audit_log RLS policy so workspace admins can also see rows
-- where their workspace was *affected* by a system task — not just rows
-- whose `workspace_id` is theirs. Without this, the new column is
-- write-only from the workspace's point of view.
DROP POLICY IF EXISTS ws_isolation ON audit_log;
CREATE POLICY ws_isolation ON audit_log
    USING (
        workspace_id = current_setting('app.workspace_id', true)::uuid
        OR affected_workspace_id = current_setting('app.workspace_id', true)::uuid
    )
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);

-- 4.8 Prompt templates: per-workspace override.
ALTER TABLE prompt_templates ADD COLUMN IF NOT EXISTS workspace_id UUID;
-- Unique: (name, version, workspace_id) — allows workspace-specific overrides
-- while keeping global templates (workspace_id IS NULL) as fallback.
CREATE UNIQUE INDEX IF NOT EXISTS uq_prompt_ws_name_version
    ON prompt_templates (name, version, workspace_id)
    WHERE workspace_id IS NOT NULL;

-- 4.10 Dashboard share token expiry.
ALTER TABLE dashboards ADD COLUMN IF NOT EXISTS share_expires_at TIMESTAMPTZ;

-- 4.2 API key identity tracking.
-- Previously all API-key requests were attributed to "system:api-key".
-- Now each key has a label for audit-trail attribution.
CREATE TABLE IF NOT EXISTS api_keys (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    label TEXT NOT NULL,
    key_hash BYTEA NOT NULL,
    created_by TEXT NOT NULL,
    workspace_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    revoked_at TIMESTAMPTZ
);
CREATE INDEX IF NOT EXISTS idx_api_keys_hash ON api_keys (key_hash);

-- RLS for api_keys.
--
-- Workspace-scoped keys obey isolation; global keys (workspace_id IS NULL)
-- are reserved for platform-admin operations and are *not* visible to a
-- normal workspace session. The auth middleware does its hash lookup
-- under SYSTEM_BYPASS so login still works for global keys; the
-- restriction here only governs admin UI listings.
ALTER TABLE api_keys ENABLE ROW LEVEL SECURITY;
ALTER TABLE api_keys FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON api_keys
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON api_keys
    USING (current_setting('app.system_bypass', true) = 'true');
