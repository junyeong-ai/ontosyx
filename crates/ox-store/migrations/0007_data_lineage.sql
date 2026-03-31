-- ============================================================
-- 0007: Data Lineage — track provenance of graph data
-- ============================================================
-- Records which source table/column produced which graph label,
-- who loaded it, when, and how many records were processed.
-- Enables full traceability from graph node back to source row.
-- ============================================================

CREATE TABLE IF NOT EXISTS data_lineage (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id        UUID NOT NULL REFERENCES workspaces(id),
    project_id          UUID,           -- design project that produced this load
    graph_label         TEXT NOT NULL,   -- 'Customer', 'PURCHASED', etc.
    graph_element_type  TEXT NOT NULL CHECK (graph_element_type IN ('node', 'edge')),
    source_type         TEXT NOT NULL,   -- 'postgresql', 'csv', 'json', 'manual'
    source_name         TEXT NOT NULL,   -- connection string or filename
    source_table        TEXT,            -- schema-qualified table name
    source_columns      TEXT[],          -- mapped source columns
    load_plan_hash      TEXT,            -- hash of the compiled load statement
    record_count        BIGINT NOT NULL DEFAULT 0,
    loaded_by           UUID REFERENCES users(id),
    started_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at        TIMESTAMPTZ,
    status              TEXT NOT NULL DEFAULT 'running' CHECK (status IN ('running', 'completed', 'failed')),
    error_message       TEXT
);

-- Indexes
CREATE INDEX IF NOT EXISTS idx_lineage_workspace  ON data_lineage(workspace_id, started_at DESC);
CREATE INDEX IF NOT EXISTS idx_lineage_project    ON data_lineage(project_id) WHERE project_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_lineage_label      ON data_lineage(graph_label, graph_element_type);
CREATE INDEX IF NOT EXISTS idx_lineage_source     ON data_lineage(source_name, source_table);

-- RLS
ALTER TABLE data_lineage ENABLE ROW LEVEL SECURITY;
ALTER TABLE data_lineage FORCE ROW LEVEL SECURITY;

CREATE POLICY ws_isolation ON data_lineage
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid);

CREATE POLICY system_bypass ON data_lineage
    USING (current_setting('app.system_bypass', true) = 'true');

ALTER TABLE data_lineage
    ALTER COLUMN workspace_id
    SET DEFAULT current_setting('app.workspace_id', true)::uuid;
