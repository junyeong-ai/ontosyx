-- Migration 0008 — Saved query patterns (canvas layout persistence)
--
-- Rationale: `/api/query/pattern/compile` discards canvas-only fields
-- (PatternNode.position, LayoutHints.zoom / pan) because QueryIR is
-- canvas-agnostic by design. To let users reopen an in-progress canvas
-- with their layout intact, we need a resource that persists the
-- PatternIR itself rather than the compiled QueryIR.
--
-- Shape mirrors `saved_reports` and `workbench_perspectives`: workspace
-- isolation via RLS, (user, ontology, name) uniqueness so the UI can
-- treat names as a stable handle, JSONB for the PatternIR payload.

BEGIN;

CREATE TABLE saved_query_patterns (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    user_id text NOT NULL,
    ontology_id text NOT NULL,
    name text NOT NULL,
    description text,
    pattern_ir jsonb NOT NULL,
    created_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    updated_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    workspace_id uuid DEFAULT (current_setting('app.workspace_id', true))::uuid NOT NULL,
    CONSTRAINT saved_query_patterns_pkey PRIMARY KEY (id),
    CONSTRAINT saved_query_patterns_user_ontology_name_key
        UNIQUE (user_id, ontology_id, name)
);

ALTER TABLE saved_query_patterns ENABLE ROW LEVEL SECURITY;
ALTER TABLE saved_query_patterns FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON saved_query_patterns
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON saved_query_patterns
    USING (current_setting('app.system_bypass', true) = 'true');

-- Listing by (user, ontology) sorted by most recently edited is the
-- dominant read path — a single composite index serves it.
CREATE INDEX idx_saved_query_patterns_user_ontology
    ON saved_query_patterns (user_id, ontology_id, updated_at DESC);

COMMIT;
