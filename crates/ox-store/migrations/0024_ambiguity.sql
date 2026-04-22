-- ============================================================================
-- 0024_ambiguity.sql
--
-- Closed-loop ambiguity resolver storage.
--
-- `ambiguity_contexts` — one row per detected ambiguous column. Natural
--     key (workspace_id, source_id, relation, column) enforces "one live
--     context per column". Re-running source analysis replaces the row
--     and writes a new `detection_source_hash`; resolutions whose
--     `context_source_hash` diverges are stale.
--
-- `ambiguity_resolutions` — append-only log. At most one non-revoked
--     resolution is "active" per context (enforced by a partial unique
--     index on active rows). Superseding a resolution writes a new row
--     with `supersedes` pointing back, preserving history for audit
--     trails + undo.
-- ============================================================================

CREATE TABLE ambiguity_contexts (
    id                      UUID PRIMARY KEY,
    workspace_id            UUID NOT NULL,
    source_id               TEXT NOT NULL,
    relation                TEXT NOT NULL,
    column_name             TEXT NOT NULL,
    kind                    TEXT NOT NULL
        CHECK (kind IN ('numeric_code', 'opaque_short_code', 'overloaded_name')),
    sample_values           JSONB NOT NULL,
    distinct_estimate       BIGINT,
    nullable                BOOLEAN NOT NULL DEFAULT false,
    clarification_prompt    TEXT NOT NULL,
    detection_source_hash   TEXT NOT NULL,
    repo_hint               JSONB,
    detected_at             TIMESTAMPTZ NOT NULL DEFAULT now(),

    UNIQUE (workspace_id, source_id, relation, column_name)
);

ALTER TABLE ambiguity_contexts ENABLE ROW LEVEL SECURITY;
ALTER TABLE ambiguity_contexts FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON ambiguity_contexts
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON ambiguity_contexts
    USING (current_setting('app.system_bypass', true) = 'true');

CREATE INDEX idx_ambiguity_contexts_source
    ON ambiguity_contexts (workspace_id, source_id);
CREATE INDEX idx_ambiguity_contexts_column
    ON ambiguity_contexts (workspace_id, source_id, relation, column_name);


CREATE TABLE ambiguity_resolutions (
    id                      UUID PRIMARY KEY,
    workspace_id            UUID NOT NULL,
    context_id              UUID NOT NULL
        REFERENCES ambiguity_contexts(id) ON DELETE CASCADE,
    context_source_hash     TEXT NOT NULL,
    mapping                 JSONB NOT NULL,
    resolved_at             TIMESTAMPTZ NOT NULL DEFAULT now(),
    resolved_by_user_id     UUID,
    supersedes_id           UUID REFERENCES ambiguity_resolutions(id),
    revoked_at              TIMESTAMPTZ
);

ALTER TABLE ambiguity_resolutions ENABLE ROW LEVEL SECURITY;
ALTER TABLE ambiguity_resolutions FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON ambiguity_resolutions
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON ambiguity_resolutions
    USING (current_setting('app.system_bypass', true) = 'true');

-- Partial unique index: at most one ACTIVE resolution per context.
-- A superseding resolution revokes the old one in the same transaction
-- (handled by the store impl) so this invariant holds without an
-- explicit trigger.
CREATE UNIQUE INDEX uq_ambiguity_resolutions_active
    ON ambiguity_resolutions (context_id)
    WHERE revoked_at IS NULL;

CREATE INDEX idx_ambiguity_resolutions_context
    ON ambiguity_resolutions (context_id, resolved_at DESC);
