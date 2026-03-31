-- ============================================================
-- 0010: Fine-grained ACLs — attribute-based access control
-- ============================================================
-- ABAC (Attribute-Based Access Control) policies for column-level
-- masking and property-level deny on graph query results.
--
-- Policies specify: who (subject) can see what (resource/property)
-- with what restrictions (mask/deny/allow).
--
-- Evaluation order: deny > mask > allow (most restrictive wins).
-- ============================================================

CREATE TABLE IF NOT EXISTS acl_policies (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id    UUID NOT NULL REFERENCES workspaces(id),
    name            TEXT NOT NULL,
    description     TEXT,
    -- Subject: who this policy applies to
    subject_type    TEXT NOT NULL CHECK (subject_type IN ('role', 'user', 'workspace_role')),
    subject_value   TEXT NOT NULL,  -- 'viewer', user UUID, or 'member'
    -- Resource: what this policy applies to
    resource_type   TEXT NOT NULL CHECK (resource_type IN ('node_label', 'edge_label', 'all')),
    resource_value  TEXT,           -- 'Customer', 'PURCHASED', null for 'all'
    -- Action: how properties are restricted
    action          TEXT NOT NULL CHECK (action IN ('mask', 'deny', 'allow')),
    -- Properties affected (null = all properties on the resource)
    properties      TEXT[],         -- ['email', 'phone', 'ssn'] or null
    -- Mask pattern (only for action='mask')
    mask_pattern    TEXT DEFAULT '***',  -- replacement string
    -- Priority: higher = evaluated first (for conflict resolution)
    priority        INTEGER NOT NULL DEFAULT 0,
    is_active       BOOLEAN NOT NULL DEFAULT true,
    created_by      UUID REFERENCES users(id),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Indexes
CREATE INDEX IF NOT EXISTS idx_acl_workspace     ON acl_policies(workspace_id, is_active);
CREATE INDEX IF NOT EXISTS idx_acl_subject       ON acl_policies(subject_type, subject_value);
CREATE INDEX IF NOT EXISTS idx_acl_resource      ON acl_policies(resource_type, resource_value);
CREATE INDEX IF NOT EXISTS idx_acl_priority      ON acl_policies(workspace_id, priority DESC);

-- RLS
ALTER TABLE acl_policies ENABLE ROW LEVEL SECURITY;
ALTER TABLE acl_policies FORCE ROW LEVEL SECURITY;

CREATE POLICY ws_isolation ON acl_policies
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid);

CREATE POLICY system_bypass ON acl_policies
    USING (current_setting('app.system_bypass', true) = 'true');

ALTER TABLE acl_policies
    ALTER COLUMN workspace_id
    SET DEFAULT current_setting('app.workspace_id', true)::uuid;
