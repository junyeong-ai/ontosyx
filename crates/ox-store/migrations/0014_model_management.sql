-- ============================================================
-- 0014: Model Management — runtime LLM model configuration
-- ============================================================
-- Enables per-workspace model configs and operation-based routing rules.
-- Replaces static TOML-based model configuration with DB-backed
-- runtime-configurable models.
-- ============================================================

CREATE TABLE IF NOT EXISTS model_configs (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id    UUID REFERENCES workspaces(id) ON DELETE CASCADE,
    name            TEXT NOT NULL,
    provider        TEXT NOT NULL,
    model_id        TEXT NOT NULL,
    max_tokens      INT NOT NULL DEFAULT 8192,
    temperature     REAL,
    timeout_secs    INT NOT NULL DEFAULT 300,
    cost_per_1m_input  DOUBLE PRECISION,
    cost_per_1m_output DOUBLE PRECISION,
    daily_budget_usd   DOUBLE PRECISION,
    priority        INT NOT NULL DEFAULT 0,
    enabled         BOOLEAN NOT NULL DEFAULT true,
    api_key_env     TEXT,
    region          TEXT,
    base_url        TEXT,
    provider_meta   JSONB NOT NULL DEFAULT '{}',
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Unique constraint: one config per name per scope (workspace or global)
CREATE UNIQUE INDEX IF NOT EXISTS idx_model_configs_scope_name
    ON model_configs(COALESCE(workspace_id, '00000000-0000-0000-0000-000000000000'), name);

CREATE TABLE IF NOT EXISTS model_routing_rules (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id    UUID REFERENCES workspaces(id) ON DELETE CASCADE,
    operation       TEXT NOT NULL,
    model_config_id UUID NOT NULL REFERENCES model_configs(id) ON DELETE CASCADE,
    priority        INT NOT NULL DEFAULT 0,
    enabled         BOOLEAN NOT NULL DEFAULT true,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_routing_lookup
    ON model_routing_rules(
        COALESCE(workspace_id, '00000000-0000-0000-0000-000000000000'),
        operation, priority DESC
    );

-- RLS: global configs (workspace_id IS NULL) visible to all
ALTER TABLE model_configs ENABLE ROW LEVEL SECURITY;
ALTER TABLE model_configs FORCE ROW LEVEL SECURITY;

CREATE POLICY ws_or_global ON model_configs
    USING (workspace_id IS NULL
        OR workspace_id = current_setting('app.workspace_id', true)::uuid);

CREATE POLICY system_bypass ON model_configs
    USING (current_setting('app.system_bypass', true) = 'true');

ALTER TABLE model_routing_rules ENABLE ROW LEVEL SECURITY;
ALTER TABLE model_routing_rules FORCE ROW LEVEL SECURITY;

CREATE POLICY ws_or_global ON model_routing_rules
    USING (workspace_id IS NULL
        OR workspace_id = current_setting('app.workspace_id', true)::uuid);

CREATE POLICY system_bypass ON model_routing_rules
    USING (current_setting('app.system_bypass', true) = 'true');
