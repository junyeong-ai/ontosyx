-- ============================================================
-- 0009: Data Quality Framework — declarative quality rules
--       with automated evaluation and result tracking
-- ============================================================
-- Quality rules can check: completeness, uniqueness, freshness,
-- consistency, and custom Cypher conditions on graph data.
-- Workspace-scoped via RLS (same pattern as all other tables).
-- ============================================================

CREATE TABLE IF NOT EXISTS quality_rules (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id    UUID NOT NULL REFERENCES workspaces(id),
    name            TEXT NOT NULL,
    description     TEXT,
    rule_type       TEXT NOT NULL CHECK (rule_type IN ('completeness', 'uniqueness', 'freshness', 'consistency', 'custom')),
    target_label    TEXT NOT NULL,       -- graph node/edge label this rule applies to
    target_property TEXT,                -- specific property (null = applies to all nodes of label)
    threshold       NUMERIC(5,2) NOT NULL DEFAULT 95.0,  -- pass threshold (e.g., 95% completeness)
    cypher_check    TEXT,                -- custom Cypher query (for 'custom' type)
    severity        TEXT NOT NULL DEFAULT 'warning' CHECK (severity IN ('critical', 'warning', 'info')),
    is_active       BOOLEAN NOT NULL DEFAULT true,
    created_by      UUID REFERENCES users(id),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS quality_results (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id    UUID NOT NULL REFERENCES workspaces(id),
    rule_id         UUID NOT NULL REFERENCES quality_rules(id) ON DELETE CASCADE,
    passed          BOOLEAN NOT NULL,
    actual_value    NUMERIC(10,4),    -- measured value (e.g., 92.5% completeness)
    details         JSONB DEFAULT '{}',
    evaluated_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Indexes for common access patterns
CREATE INDEX IF NOT EXISTS idx_quality_rules_workspace   ON quality_rules(workspace_id);
CREATE INDEX IF NOT EXISTS idx_quality_rules_label       ON quality_rules(target_label);
CREATE INDEX IF NOT EXISTS idx_quality_rules_active      ON quality_rules(is_active, severity);
CREATE INDEX IF NOT EXISTS idx_quality_results_rule      ON quality_results(rule_id, evaluated_at DESC);
CREATE INDEX IF NOT EXISTS idx_quality_results_workspace ON quality_results(workspace_id, evaluated_at DESC);

-- RLS policies (same pattern as other tables)
ALTER TABLE quality_rules ENABLE ROW LEVEL SECURITY;
ALTER TABLE quality_rules FORCE ROW LEVEL SECURITY;

CREATE POLICY ws_isolation ON quality_rules
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid);

CREATE POLICY system_bypass ON quality_rules
    USING (current_setting('app.system_bypass', true) = 'true');

ALTER TABLE quality_rules
    ALTER COLUMN workspace_id
    SET DEFAULT current_setting('app.workspace_id', true)::uuid;

ALTER TABLE quality_results ENABLE ROW LEVEL SECURITY;
ALTER TABLE quality_results FORCE ROW LEVEL SECURITY;

CREATE POLICY ws_isolation ON quality_results
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid);

CREATE POLICY system_bypass ON quality_results
    USING (current_setting('app.system_bypass', true) = 'true');

ALTER TABLE quality_results
    ALTER COLUMN workspace_id
    SET DEFAULT current_setting('app.workspace_id', true)::uuid;
