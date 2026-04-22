-- ============================================================================
-- 0011_data_sources.sql
--
-- Persistence for federation (VOL) adapter configurations. The admin
-- CRUD endpoints (POST/GET/DELETE /api/admin/federation/adapters,
-- slice W2) previously wrote only to an in-memory
-- `InMemoryAdapterResolver`; restarting the server dropped every
-- registration. This table is the durable source of truth that a
-- future AppState bootstrap will hydrate into the live resolver.
--
-- `source_id` is the opaque string the planner resolves via
-- `ObjectMappingDef::source_id`. It is author-chosen, must be stable
-- (ontology mappings reference it), and is unique per workspace.
-- Different workspaces may reuse the same `source_id` label without
-- collision.
--
-- `kind` describes the adapter flavour (`csv`, `json`, later
-- `postgres`, `mysql`, `duckdb`, `snowflake`, ...). The planner does
-- not read this column; it is the hint the bootstrap path consults
-- to know which adapter factory to apply to the `config` payload.
--
-- `config` is the adapter-specific payload, stored as JSONB so that
-- the future Postgres / Snowflake shapes (connection strings,
-- secret-manager refs, warehouse names) fit without another
-- migration. Today's slice only writes `{"data": "..."}` for
-- inline CSV / JSON payloads.
-- ============================================================================

CREATE TABLE data_sources (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    -- Defaults to the task-local `app.workspace_id` the HTTP middleware
    -- sets on the connection. Inserts therefore do not have to name
    -- workspace_id explicitly — the session setting carries it. Same
    -- pattern saved_ontologies (see 0001_schema.sql) uses.
    workspace_id  UUID NOT NULL DEFAULT (current_setting('app.workspace_id', true))::uuid
                       REFERENCES workspaces(id) ON DELETE CASCADE,
    source_id     TEXT NOT NULL,
    kind          TEXT NOT NULL,
    config        JSONB NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),

    -- Within a workspace, `source_id` must be unique — ontology
    -- mappings reference it by that string and the planner would
    -- not know which adapter to resolve on collision.
    CONSTRAINT data_sources_ws_source_unique UNIQUE (workspace_id, source_id),

    -- Kinds the slice-W3 bootstrap path can handle. Extended as
    -- new adapter factories are wired in.
    CONSTRAINT data_sources_kind_allowed
        CHECK (kind IN ('csv', 'json'))
);

CREATE INDEX data_sources_workspace_idx ON data_sources (workspace_id);

-- RLS: identical four-statement boilerplate every workspace-scoped
-- table in this schema uses. See 0004_rls.sql for the parent set.
-- The FORCE line is load-bearing — without it the table owner role
-- bypasses ws_isolation, defeating the whole gate.
ALTER TABLE data_sources ENABLE ROW LEVEL SECURITY;
ALTER TABLE data_sources FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON data_sources
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON data_sources
    USING (current_setting('app.system_bypass', true) = 'true');
