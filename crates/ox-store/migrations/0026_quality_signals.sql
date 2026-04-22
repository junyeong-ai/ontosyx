-- ============================================================================
-- 0026_quality_signals.sql
--
-- Per-query signal log + per-type last-used tracker that feed the
-- "six windows" quality dashboard.
--
-- `query_execution_signals` (1 row per successful query):
--   anchor / glossary / ambiguity / SHACL / hash / touched types
--   written fire-and-forget AFTER the runtime returns — a signal
--   write failure never blocks the user-facing query.
--
-- `ontology_type_last_used` (1 row per type_id):
--   upserted on every signal write with the touched type_ids. The
--   stale-concept scan reads this table directly so the aggregator
--   isn't scanning the full signal log every time.
-- ============================================================================

CREATE TABLE query_execution_signals (
    execution_id                UUID PRIMARY KEY
        REFERENCES query_executions(id) ON DELETE CASCADE,
    workspace_id                UUID NOT NULL,
    captured_at                 TIMESTAMPTZ NOT NULL DEFAULT now(),

    -- Anchor layer
    anchor_top_score            DOUBLE PRECISION,
    anchor_hit_kinds            TEXT[] NOT NULL DEFAULT '{}',

    -- Glossary layer
    glossary_term_hits          UUID[] NOT NULL DEFAULT '{}',

    -- Ambiguity layer
    ambiguity_resolution_ids    UUID[] NOT NULL DEFAULT '{}',
    ambiguity_was_clarified     BOOLEAN NOT NULL DEFAULT false,

    -- SHACL layer
    shacl_passed                BOOLEAN NOT NULL,
    shacl_failure_kind          TEXT
        CHECK (shacl_failure_kind IS NULL OR shacl_failure_kind IN (
            'cardinality_violation',
            'measure_group_by',
            'unknown_coded_value',
            'mandatory_property_missing',
            'temporal_grain_mismatch',
            'other'
        )),

    -- Reproducibility layer
    query_ir_normalized_hash    TEXT NOT NULL,

    -- Stale-concept layer
    referenced_type_ids         UUID[] NOT NULL DEFAULT '{}'
);

ALTER TABLE query_execution_signals ENABLE ROW LEVEL SECURITY;
ALTER TABLE query_execution_signals FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON query_execution_signals
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON query_execution_signals
    USING (current_setting('app.system_bypass', true) = 'true');

-- Time-windowed aggregation is the hot path — composite index on
-- (workspace_id, captured_at DESC) keeps the 7d / 30d / 90d scans
-- index-only. GIN on referenced_type_ids lets the stale-scan `= ANY`
-- lookups stay cheap as the log grows.
CREATE INDEX idx_query_execution_signals_window
    ON query_execution_signals (workspace_id, captured_at DESC);
CREATE INDEX idx_query_execution_signals_type_ids
    ON query_execution_signals USING GIN (referenced_type_ids);
CREATE INDEX idx_query_execution_signals_reproducibility
    ON query_execution_signals (workspace_id, query_ir_normalized_hash);


CREATE TABLE ontology_type_last_used (
    workspace_id        UUID NOT NULL,
    type_id             UUID NOT NULL,
    type_kind           TEXT NOT NULL,
    last_used_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    use_count_7d        INT  NOT NULL DEFAULT 0,
    use_count_30d       INT  NOT NULL DEFAULT 0,
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (workspace_id, type_id)
);

ALTER TABLE ontology_type_last_used ENABLE ROW LEVEL SECURITY;
ALTER TABLE ontology_type_last_used FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON ontology_type_last_used
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON ontology_type_last_used
    USING (current_setting('app.system_bypass', true) = 'true');

CREATE INDEX idx_ontology_type_last_used_stale
    ON ontology_type_last_used (workspace_id, last_used_at);
