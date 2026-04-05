-- Load checkpoints: watermark-based incremental data loading state.
-- Tracks the last successfully loaded watermark value per (project, source_table, graph_label).

CREATE TABLE load_checkpoints (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id    UUID NOT NULL,
    project_id      UUID NOT NULL,
    source_table    VARCHAR(255) NOT NULL,
    graph_label     VARCHAR(255) NOT NULL,
    watermark_column VARCHAR(255) NOT NULL,
    watermark_value TEXT NOT NULL,
    record_count    BIGINT NOT NULL DEFAULT 0,
    loaded_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(workspace_id, project_id, source_table, graph_label)
);

-- RLS: workspace isolation
ALTER TABLE load_checkpoints ENABLE ROW LEVEL SECURITY;
CREATE POLICY load_checkpoints_workspace ON load_checkpoints
    USING (workspace_id = current_setting('app.workspace_id')::uuid);
