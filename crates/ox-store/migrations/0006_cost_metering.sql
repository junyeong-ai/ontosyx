-- ============================================================
-- 0006: Cost Metering — track LLM, compute, and storage usage
-- ============================================================
-- Per-workspace usage records for billing, budgeting, and cost visibility.
-- High-volume table: expects 100+ records per active user per day.
-- ============================================================

CREATE TABLE IF NOT EXISTS usage_records (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id    UUID NOT NULL REFERENCES workspaces(id),
    user_id         UUID REFERENCES users(id),
    resource_type   TEXT NOT NULL,      -- 'llm', 'graph_query', 'analysis', 'storage', 'embedding'
    provider        TEXT,               -- 'anthropic', 'bedrock', 'neo4j', 'onnx'
    model           TEXT,               -- 'claude-sonnet-4-6', 'claude-haiku-4-5', etc.
    operation       TEXT,               -- 'design', 'refine', 'translate_query', 'explain', 'embed'
    input_tokens    BIGINT DEFAULT 0,
    output_tokens   BIGINT DEFAULT 0,
    duration_ms     BIGINT DEFAULT 0,
    cost_usd        NUMERIC(12,6) DEFAULT 0,
    metadata        JSONB DEFAULT '{}',
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Partitioning-friendly indexes for time-range queries
CREATE INDEX IF NOT EXISTS idx_usage_workspace_time ON usage_records(workspace_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_usage_user_time      ON usage_records(user_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_usage_resource_type   ON usage_records(resource_type, created_at DESC);

-- RLS
ALTER TABLE usage_records ENABLE ROW LEVEL SECURITY;
ALTER TABLE usage_records FORCE ROW LEVEL SECURITY;

CREATE POLICY ws_isolation ON usage_records
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid);

CREATE POLICY system_bypass ON usage_records
    USING (current_setting('app.system_bypass', true) = 'true');

ALTER TABLE usage_records
    ALTER COLUMN workspace_id
    SET DEFAULT current_setting('app.workspace_id', true)::uuid;
