-- 0003: Verification system, HITL tool approvals, and config fixes
--
-- Phase 2: ontology_verifications — per-element verification tracking
-- Phase 3: tool_approvals — HITL tool review decisions
-- Phase 6: batch_size config seed + duplicate index cleanup

-- ---------------------------------------------------------------------------
-- ontology_verifications — element-level verification tracking
--
-- Design: ontology_id is NOT a FK to saved_ontologies — verifications are
-- keyed by the OntologyIR.id string (which may be a draft ontology, not yet
-- saved). Orphaned rows are acceptable for audit trail; periodic cleanup
-- can purge verifications for deleted ontologies if needed.
-- ---------------------------------------------------------------------------

CREATE TABLE ontology_verifications (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    ontology_id         VARCHAR(255) NOT NULL,
    element_id          VARCHAR(255) NOT NULL,
    element_kind        VARCHAR(50) NOT NULL,  -- 'node' | 'edge' | 'property'
    verified_by         UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    review_notes        TEXT,
    invalidated_at      TIMESTAMPTZ,
    invalidation_reason TEXT,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Only one active verification per element per user
CREATE UNIQUE INDEX idx_verifications_active
    ON ontology_verifications(ontology_id, element_id, verified_by)
    WHERE invalidated_at IS NULL;

-- Fast lookup: all active verifications for an ontology
CREATE INDEX idx_verifications_ontology
    ON ontology_verifications(ontology_id)
    WHERE invalidated_at IS NULL;

-- ---------------------------------------------------------------------------
-- tool_approvals — HITL tool review decisions for agent sessions
-- ---------------------------------------------------------------------------

CREATE TABLE tool_approvals (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    session_id      UUID NOT NULL REFERENCES agent_sessions(id) ON DELETE CASCADE,
    tool_call_id    VARCHAR(255) NOT NULL,
    approved        BOOLEAN NOT NULL,
    reason          TEXT,
    modified_input  JSONB,
    user_id         VARCHAR(255) NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(session_id, tool_call_id)
);

CREATE INDEX idx_tool_approvals_session
    ON tool_approvals(session_id, created_at DESC);

-- ---------------------------------------------------------------------------
-- Config seeds
-- ---------------------------------------------------------------------------

INSERT INTO system_config (category, key, value, data_type, description)
VALUES ('design', 'batch_size', '15', 'int', 'Number of tables per LLM batch in multi-batch design')
ON CONFLICT DO NOTHING;

INSERT INTO system_config (category, key, value, data_type, description)
VALUES ('timeouts', 'tool_review_secs', '120', 'int', 'HITL tool review approval timeout in seconds')
ON CONFLICT DO NOTHING;

-- ---------------------------------------------------------------------------
-- Drop duplicate indices from 0002 that already exist in 0001
-- ---------------------------------------------------------------------------

DROP INDEX IF EXISTS idx_scheduled_tasks_due;       -- duplicate of idx_scheduled_tasks_next_run
DROP INDEX IF EXISTS idx_memory_metadata_source;    -- duplicate of idx_memory_source
