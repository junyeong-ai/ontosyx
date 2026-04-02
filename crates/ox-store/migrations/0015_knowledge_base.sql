-- ============================================================
-- Knowledge Base — workspace-scoped, ontology-version-aware
-- Failure-driven learning: corrections from query errors + admin hints
-- ============================================================

BEGIN;

SET LOCAL app.system_bypass = 'true';

CREATE TABLE knowledge_entries (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id        UUID NOT NULL DEFAULT current_setting('app.workspace_id', true)::uuid
                        REFERENCES workspaces(id) ON DELETE CASCADE,
    ontology_name       VARCHAR(255) NOT NULL,
    ontology_version_min INT NOT NULL DEFAULT 1,
    ontology_version_max INT,
    kind                VARCHAR(50) NOT NULL,
    status              VARCHAR(20) NOT NULL DEFAULT 'draft',
    confidence          DOUBLE PRECISION NOT NULL DEFAULT 0.5
                        CHECK (confidence >= 0.0 AND confidence <= 1.0),
    title               VARCHAR(500) NOT NULL,
    content             TEXT NOT NULL,
    structured_data     JSONB NOT NULL DEFAULT '{}',
    embedding           vector(1024),
    version_checked     INT NOT NULL DEFAULT 1,
    content_hash        VARCHAR(64) NOT NULL,
    source_execution_ids UUID[] NOT NULL DEFAULT '{}',
    source_session_id   UUID,
    affected_labels     TEXT[] NOT NULL DEFAULT '{}',
    created_by          VARCHAR(255) NOT NULL,
    reviewed_by         UUID REFERENCES users(id) ON DELETE SET NULL,
    reviewed_at         TIMESTAMPTZ,
    review_notes        TEXT,
    use_count           BIGINT NOT NULL DEFAULT 0,
    last_used_at        TIMESTAMPTZ,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Dedup: one knowledge entry per ontology+content combination
CREATE UNIQUE INDEX uq_knowledge_content_hash
    ON knowledge_entries (workspace_id, ontology_name, content_hash);

-- Primary RAG: label-based GIN lookup
CREATE INDEX idx_knowledge_affected_labels
    ON knowledge_entries USING GIN (affected_labels);

-- Active knowledge filtered by ontology + status
CREATE INDEX idx_knowledge_ontology_active
    ON knowledge_entries (workspace_id, ontology_name, status)
    WHERE status IN ('approved', 'draft');

-- Confidence ranking
CREATE INDEX idx_knowledge_confidence
    ON knowledge_entries (workspace_id, confidence DESC)
    WHERE status = 'approved';

-- Compound workspace FK
CREATE UNIQUE INDEX uq_knowledge_ws_id
    ON knowledge_entries (workspace_id, id);

-- Workspace-scoped index
CREATE INDEX idx_knowledge_workspace
    ON knowledge_entries (workspace_id);

-- RLS
ALTER TABLE knowledge_entries ENABLE ROW LEVEL SECURITY;
ALTER TABLE knowledge_entries FORCE ROW LEVEL SECURITY;

CREATE POLICY ws_isolation ON knowledge_entries
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid);

CREATE POLICY system_bypass ON knowledge_entries
    USING (current_setting('app.system_bypass', true) = 'true');

COMMIT;
