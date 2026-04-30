-- ============================================================================
-- Draft cluster checkpoints — ADR-0027
--
-- `design_ontology_batch` runs the LLM design call N times across
-- N table clusters per design pass. A transient failure on cluster K
-- previously discarded clusters 0..K's output and forced the caller
-- to start from scratch — every prior LLM call wasted. This table
-- caches one completed cluster output keyed by a deterministic
-- `(workspace_id, project_id, source_id, signature)` natural key
-- (signature = SHA-256 from `ClusterSignature::from_cluster`,
-- folding tables + FKs + prompt template hash). A re-run with the
-- same signature replays from cache and skips the LLM call;
-- `expires_at` lets the daily cleanup cron drop stale rows so
-- abandoned design sessions don't accumulate.
-- ============================================================================

CREATE TABLE draft_cluster_checkpoints (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id uuid NOT NULL,
    project_id uuid NOT NULL,
    source_id text NOT NULL,
    signature text NOT NULL,
    cluster_id integer NOT NULL,
    output jsonb NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL,
    UNIQUE (workspace_id, project_id, source_id, signature)
);

ALTER TABLE draft_cluster_checkpoints ENABLE ROW LEVEL SECURITY;
ALTER TABLE draft_cluster_checkpoints FORCE ROW LEVEL SECURITY;

CREATE POLICY ws_isolation ON draft_cluster_checkpoints
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);

CREATE POLICY system_bypass ON draft_cluster_checkpoints
    USING (current_setting('app.system_bypass', true) = 'true')
    WITH CHECK (current_setting('app.system_bypass', true) = 'true');

-- Cleanup cron sweep — index on `expires_at` so the daily DELETE
-- is O(log n) regardless of total checkpoint count.
CREATE INDEX idx_draft_cluster_checkpoints_expired
    ON draft_cluster_checkpoints (expires_at);

-- Per-project listing (debug + telemetry surface). The unique
-- constraint already covers signature-keyed lookups, so this index
-- targets the "show me all checkpoints for project X" path.
CREATE INDEX idx_draft_cluster_checkpoints_project
    ON draft_cluster_checkpoints (project_id, created_at DESC);
