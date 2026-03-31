-- Ontosyx application database schema
-- Single canonical migration: all tables in their final form.

-- ---------------------------------------------------------------------------
-- Extensions
-- ---------------------------------------------------------------------------

CREATE EXTENSION IF NOT EXISTS vector;

-- ---------------------------------------------------------------------------
-- users — OIDC-based authentication with role-based access control
-- ---------------------------------------------------------------------------

CREATE TABLE users (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email           TEXT NOT NULL UNIQUE,
    name            TEXT,
    picture         TEXT,
    provider        TEXT NOT NULL,
    provider_sub    TEXT NOT NULL,
    role            TEXT NOT NULL DEFAULT 'designer',
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_login_at   TIMESTAMPTZ,
    UNIQUE(provider, provider_sub)
);

-- ---------------------------------------------------------------------------
-- saved_ontologies — persistent ontology definitions
-- ---------------------------------------------------------------------------

CREATE TABLE saved_ontologies (
    id              UUID PRIMARY KEY,
    name            TEXT NOT NULL,
    description     TEXT,
    version         INT NOT NULL,
    ontology_ir     JSONB NOT NULL,
    created_by      TEXT NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(name, version)
);

-- ---------------------------------------------------------------------------
-- query_executions — NL query → ontology → compiled query → results
-- ---------------------------------------------------------------------------

CREATE TABLE query_executions (
    id                  UUID PRIMARY KEY,
    user_id             TEXT NOT NULL,
    question            TEXT NOT NULL,
    ontology_id         TEXT NOT NULL,
    ontology_version    INT NOT NULL,
    saved_ontology_id   UUID REFERENCES saved_ontologies(id) ON DELETE RESTRICT,
    ontology_snapshot   JSONB,
    query_ir            JSONB NOT NULL,
    compiled_target     TEXT NOT NULL,
    compiled_query      TEXT NOT NULL,
    results             JSONB NOT NULL,
    widget              JSONB,
    explanation         TEXT NOT NULL,
    model               TEXT NOT NULL,
    execution_time_ms   BIGINT NOT NULL,
    query_bindings      JSONB,
    feedback            VARCHAR(10),
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT chk_ontology_source CHECK (
        saved_ontology_id IS NOT NULL OR ontology_snapshot IS NOT NULL
    )
);

CREATE INDEX idx_query_executions_user
    ON query_executions(user_id, created_at DESC);

CREATE INDEX idx_query_executions_ontology_ref
    ON query_executions(saved_ontology_id)
    WHERE saved_ontology_id IS NOT NULL;

-- ---------------------------------------------------------------------------
-- pinboard_items — pinned query executions for quick access
-- ---------------------------------------------------------------------------

CREATE TABLE pinboard_items (
    id                  UUID PRIMARY KEY,
    query_execution_id  UUID NOT NULL UNIQUE REFERENCES query_executions(id) ON DELETE CASCADE,
    user_id             TEXT NOT NULL,
    widget_spec         JSONB NOT NULL,
    title               TEXT,
    pinned_at           TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_pinboard_user
    ON pinboard_items(user_id, pinned_at DESC, id DESC);

-- ---------------------------------------------------------------------------
-- design_projects — ontology design lifecycle (analyzed → designed → completed)
-- ---------------------------------------------------------------------------

CREATE TABLE design_projects (
    id              UUID PRIMARY KEY,
    user_id         TEXT NOT NULL,
    status          TEXT NOT NULL DEFAULT 'analyzed',
    revision        INT NOT NULL DEFAULT 1,
    title           TEXT,
    source_config   JSONB NOT NULL,
    source_data     TEXT,
    source_schema   JSONB,
    source_profile  JSONB,
    source_history  JSONB NOT NULL DEFAULT '[]',
    analysis_report JSONB,
    design_options  JSONB NOT NULL DEFAULT '{}',
    source_mapping  JSONB,
    ontology        JSONB,
    quality_report  JSONB,
    saved_ontology_id UUID REFERENCES saved_ontologies(id),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    analyzed_at     TIMESTAMPTZ,
    archived_at     TIMESTAMPTZ
);

CREATE INDEX idx_design_projects_user
    ON design_projects(user_id, updated_at DESC);

CREATE INDEX idx_design_projects_updated_at_id
    ON design_projects(updated_at DESC, id DESC)
    WHERE archived_at IS NULL;

-- ---------------------------------------------------------------------------
-- ontology_snapshots — revision history for version control and rollback
-- ---------------------------------------------------------------------------

CREATE TABLE ontology_snapshots (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id  UUID NOT NULL REFERENCES design_projects(id) ON DELETE CASCADE,
    revision    INTEGER NOT NULL,
    ontology    JSONB NOT NULL,
    source_mapping JSONB,
    quality_report JSONB,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(project_id, revision)
);

CREATE INDEX idx_ontology_snapshots_project
    ON ontology_snapshots(project_id, revision DESC);

-- ---------------------------------------------------------------------------
-- workbench_perspectives — per-user graph canvas state
-- ---------------------------------------------------------------------------

CREATE TABLE workbench_perspectives (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id         TEXT NOT NULL,
    lineage_id      TEXT NOT NULL,
    topology_signature TEXT NOT NULL,
    project_id      UUID REFERENCES design_projects(id) ON DELETE SET NULL,
    name            TEXT NOT NULL DEFAULT 'Default',
    positions       JSONB NOT NULL DEFAULT '{}',
    viewport        JSONB NOT NULL DEFAULT '{"x":0,"y":0,"zoom":1}',
    filters         JSONB NOT NULL DEFAULT '{}',
    collapsed_groups JSONB NOT NULL DEFAULT '[]',
    is_default      BOOLEAN NOT NULL DEFAULT false,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (user_id, lineage_id, name)
);

CREATE INDEX idx_perspectives_user_lineage
    ON workbench_perspectives(user_id, lineage_id);

CREATE INDEX idx_perspectives_topology
    ON workbench_perspectives(user_id, topology_signature);

CREATE UNIQUE INDEX idx_perspectives_unique_default
    ON workbench_perspectives(user_id, lineage_id)
    WHERE is_default = true;

-- ---------------------------------------------------------------------------
-- memory_entries — semantic memory for agent long-term recall (pgvector)
-- ---------------------------------------------------------------------------

CREATE TABLE memory_entries (
    id                VARCHAR(255) PRIMARY KEY,
    embedding         vector(1024),
    content           TEXT NOT NULL,
    metadata          JSONB NOT NULL DEFAULT '{}',
    model_id          VARCHAR(100) NOT NULL DEFAULT 'qwen3-0.6b',
    last_accessed_at  TIMESTAMPTZ,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_memory_embedding
    ON memory_entries USING hnsw (embedding vector_cosine_ops);

CREATE INDEX idx_memory_source
    ON memory_entries ((metadata->>'source'));

CREATE INDEX idx_memory_last_accessed
    ON memory_entries (last_accessed_at)
    WHERE last_accessed_at IS NOT NULL;

-- ---------------------------------------------------------------------------
-- analysis_recipes — reusable data analysis algorithms
-- ---------------------------------------------------------------------------

CREATE TABLE analysis_recipes (
    id                  UUID PRIMARY KEY,
    name                VARCHAR(255) NOT NULL,
    description         TEXT NOT NULL,
    algorithm_type      VARCHAR(50) NOT NULL,
    code_template       TEXT NOT NULL,
    parameters          JSONB NOT NULL DEFAULT '[]',
    required_columns    JSONB NOT NULL DEFAULT '[]',
    output_description  TEXT NOT NULL DEFAULT '',
    created_by          VARCHAR(255) NOT NULL,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    version             INTEGER NOT NULL DEFAULT 1,
    status              VARCHAR(20) NOT NULL DEFAULT 'approved',
    parent_id           UUID REFERENCES analysis_recipes(id) ON DELETE SET NULL
);

CREATE INDEX idx_recipes_algorithm_type
    ON analysis_recipes (algorithm_type);

CREATE INDEX idx_recipes_parent
    ON analysis_recipes (parent_id) WHERE parent_id IS NOT NULL;

CREATE INDEX idx_recipes_status
    ON analysis_recipes (status);

-- ---------------------------------------------------------------------------
-- dashboards — persistent collection of widgets with grid layout
-- ---------------------------------------------------------------------------

CREATE TABLE dashboards (
    id          UUID PRIMARY KEY,
    user_id     VARCHAR(255) NOT NULL,
    name        VARCHAR(255) NOT NULL,
    description TEXT,
    layout      JSONB NOT NULL DEFAULT '[]',
    is_public   BOOLEAN NOT NULL DEFAULT false,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_dashboards_user
    ON dashboards (user_id, updated_at DESC);

CREATE INDEX idx_dashboards_public
    ON dashboards (updated_at DESC) WHERE is_public = true;

-- ---------------------------------------------------------------------------
-- dashboard_widgets — saved query/analysis bound to a dashboard position
-- ---------------------------------------------------------------------------

CREATE TABLE dashboard_widgets (
    id                      UUID PRIMARY KEY,
    dashboard_id            UUID NOT NULL REFERENCES dashboards(id) ON DELETE CASCADE,
    title                   VARCHAR(255) NOT NULL,
    widget_type             VARCHAR(50) NOT NULL,
    query                   TEXT,
    widget_spec             JSONB NOT NULL DEFAULT '{}',
    position                JSONB NOT NULL DEFAULT '{"x":0,"y":0,"w":6,"h":4}',
    refresh_interval_secs   INTEGER,
    thresholds              JSONB,
    last_result             JSONB,
    last_refreshed          TIMESTAMPTZ,
    created_at              TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_widgets_dashboard
    ON dashboard_widgets (dashboard_id);

-- ---------------------------------------------------------------------------
-- saved_reports — parameterized query templates for reusable analytics
-- ---------------------------------------------------------------------------

CREATE TABLE saved_reports (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id         VARCHAR(255) NOT NULL,
    ontology_id     VARCHAR(255) NOT NULL,
    title           VARCHAR(255) NOT NULL,
    description     TEXT,
    query_template  TEXT NOT NULL,
    parameters      JSONB NOT NULL DEFAULT '[]',
    widget_type     VARCHAR(50),
    is_public       BOOLEAN NOT NULL DEFAULT false,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_saved_reports_user ON saved_reports (user_id, updated_at DESC);
CREATE INDEX idx_saved_reports_ontology ON saved_reports (ontology_id);
CREATE INDEX idx_saved_reports_public ON saved_reports (updated_at DESC) WHERE is_public = true;

-- ---------------------------------------------------------------------------
-- system_config — runtime-tunable configuration
-- ---------------------------------------------------------------------------

CREATE TABLE system_config (
    category    TEXT NOT NULL,
    key         TEXT NOT NULL,
    value       TEXT NOT NULL,
    data_type   TEXT NOT NULL DEFAULT 'string',
    description TEXT NOT NULL DEFAULT '',
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (category, key)
);

-- Seed: sensible defaults for all runtime-tunable settings.

INSERT INTO system_config (category, key, value, data_type, description) VALUES
    ('llm', 'design_ontology_max_tokens',   '16384', 'int',   'Max output tokens for ontology design LLM call'),
    ('llm', 'design_ontology_temperature',  '0.0',   'float', 'Temperature for ontology design LLM call'),
    ('llm', 'refine_ontology_max_tokens',   '16384', 'int',   'Max output tokens for ontology refinement LLM call'),
    ('llm', 'refine_ontology_temperature',  '0.0',   'float', 'Temperature for ontology refinement LLM call');

INSERT INTO system_config (category, key, value, data_type, description) VALUES
    ('thresholds', 'large_schema_warning',  '50',  'int', 'Table count threshold for analysis warning + LLM compression'),
    ('thresholds', 'large_schema_gate',     '100', 'int', 'Table count threshold requiring explicit acknowledgment to design'),
    ('thresholds', 'large_ontology',        '100', 'int', 'Node count threshold for adaptive profiling/serialization'),
    ('thresholds', 'max_design_tables',     '40',  'int', 'Max tables sent to LLM for ontology design (excess auto-dropped by FK connectivity)');

INSERT INTO system_config (category, key, value, data_type, description) VALUES
    ('profiling', 'max_distinct_values',         '30', 'int', 'Max distinct values to collect per property in graph profiling'),
    ('profiling', 'large_schema_sample_values',  '5',  'int', 'Max sample values per column for large schema LLM input'),
    ('profiling', 'large_schema_value_chars',    '50', 'int', 'Max chars for min/max values in large schema LLM input'),
    ('profiling', 'large_ontology_sample_size',  '10', 'int', 'Sample size for large ontology graph profiling'),
    ('profiling', 'large_ontology_concurrency',  '4',  'int', 'Concurrency for large ontology graph profiling');

INSERT INTO system_config (category, key, value, data_type, description) VALUES
    ('timeouts', 'chat_pipeline_secs',     '60',  'int', 'Chat pipeline timeout'),
    ('timeouts', 'design_operation_secs',  '120', 'int', 'Design LLM operation timeout'),
    ('timeouts', 'refine_operation_secs',  '300', 'int', 'Refine LLM operation timeout'),
    ('timeouts', 'profiling_secs',         '60',  'int', 'Graph profiling timeout');

INSERT INTO system_config (category, key, value, data_type, description) VALUES
    ('analysis', 'max_concurrent_executions', '4',   'int', 'Maximum concurrent Python analysis executions'),
    ('analysis', 'execution_timeout_secs',    '120', 'int', 'Python analysis execution timeout in seconds');

INSERT INTO system_config (category, key, value, data_type, description) VALUES
    ('ui', 'elk_direction',      'RIGHT',      'string', 'ELK layout direction (RIGHT, DOWN, LEFT, UP)'),
    ('ui', 'elk_node_spacing',   '60',         'int',    'ELK spacing between nodes'),
    ('ui', 'elk_layer_spacing',  '100',        'int',    'ELK spacing between layers'),
    ('ui', 'elk_edge_routing',   'ORTHOGONAL', 'string', 'ELK edge routing (ORTHOGONAL, POLYLINE, SPLINES)'),
    ('ui', 'worker_timeout_ms',  '10000',      'int',    'ELK Web Worker timeout in milliseconds');

-- ---------------------------------------------------------------------------
-- prompt_templates — versioned prompt management with audit trail
-- ---------------------------------------------------------------------------

CREATE TABLE prompt_templates (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name        VARCHAR(100) NOT NULL,
    version     VARCHAR(20) NOT NULL,
    content     TEXT NOT NULL,
    variables   JSONB NOT NULL DEFAULT '[]',
    metadata    JSONB NOT NULL DEFAULT '{}',
    created_by  VARCHAR(255) NOT NULL DEFAULT 'system',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    is_active   BOOLEAN NOT NULL DEFAULT true,
    UNIQUE(name, version)
);

CREATE INDEX idx_prompt_templates_active
    ON prompt_templates (name, is_active) WHERE is_active = true;

-- ---------------------------------------------------------------------------
-- agent_sessions — execution context for replay and audit
-- ---------------------------------------------------------------------------

CREATE TABLE agent_sessions (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id             VARCHAR(255) NOT NULL,
    ontology_id         VARCHAR(255),
    prompt_hash         VARCHAR(64) NOT NULL,
    tool_schema_hash    VARCHAR(64) NOT NULL,
    model_id            VARCHAR(255) NOT NULL,
    model_config        JSONB NOT NULL DEFAULT '{}',
    user_message        TEXT NOT NULL,
    final_text          TEXT,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at        TIMESTAMPTZ
);

CREATE INDEX idx_agent_sessions_user
    ON agent_sessions (user_id, created_at DESC);

-- ---------------------------------------------------------------------------
-- agent_events — ordered event sequence for session replay
-- ---------------------------------------------------------------------------

CREATE TABLE agent_events (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    session_id  UUID NOT NULL REFERENCES agent_sessions(id) ON DELETE CASCADE,
    sequence    INTEGER NOT NULL,
    event_type  VARCHAR(50) NOT NULL,
    payload     JSONB NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(session_id, sequence)
);

CREATE INDEX idx_agent_events_session
    ON agent_events (session_id, sequence);

-- ---------------------------------------------------------------------------
-- Full-text pattern search support (trigram index for ILIKE/regex)
-- ---------------------------------------------------------------------------

CREATE EXTENSION IF NOT EXISTS pg_trgm;

CREATE INDEX idx_memory_content_trgm
    ON memory_entries USING gin (content gin_trgm_ops);

-- ---------------------------------------------------------------------------
-- pending_embeddings — retry queue for failed embedding operations
-- ---------------------------------------------------------------------------

CREATE TABLE pending_embeddings (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    content     TEXT NOT NULL,
    metadata    JSONB NOT NULL,
    retry_count INTEGER NOT NULL DEFAULT 0,
    last_error  TEXT,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_pending_embeddings_retry
    ON pending_embeddings (retry_count, created_at)
    WHERE retry_count < 3;

-- ---------------------------------------------------------------------------
-- scheduled_tasks — cron-based recipe execution schedule
-- ---------------------------------------------------------------------------

CREATE TABLE scheduled_tasks (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    recipe_id       UUID NOT NULL REFERENCES analysis_recipes(id) ON DELETE CASCADE,
    ontology_id     VARCHAR(255),
    cron_expression VARCHAR(100) NOT NULL,
    description     TEXT,
    enabled         BOOLEAN NOT NULL DEFAULT true,
    last_run_at     TIMESTAMPTZ,
    next_run_at     TIMESTAMPTZ NOT NULL,
    last_status     VARCHAR(20),
    webhook_url     TEXT,
    created_by      VARCHAR(255) NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_scheduled_tasks_next_run ON scheduled_tasks (next_run_at) WHERE enabled = true;
CREATE INDEX idx_scheduled_tasks_recipe ON scheduled_tasks (recipe_id);

-- ---------------------------------------------------------------------------
-- analysis_results — versioned recipe execution outputs with cache lookup
-- ---------------------------------------------------------------------------

CREATE TABLE analysis_results (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    recipe_id       UUID REFERENCES analysis_recipes(id) ON DELETE SET NULL,
    ontology_id     VARCHAR(255),
    input_hash      VARCHAR(64) NOT NULL,
    output          JSONB NOT NULL,
    duration_ms     BIGINT NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_analysis_results_recipe
    ON analysis_results (recipe_id, created_at DESC);

CREATE INDEX idx_analysis_results_cache
    ON analysis_results (input_hash, recipe_id);

-- ---------------------------------------------------------------------------
-- Archived project lookup (cleanup task: find archived projects past grace period)
-- ---------------------------------------------------------------------------

CREATE INDEX idx_projects_archived
    ON design_projects(archived_at) WHERE archived_at IS NOT NULL;
