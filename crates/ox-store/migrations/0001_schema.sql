-- ============================================================================
-- 0001_schema.sql
--
-- Ontosyx v1 database schema. Single-file, authored as if designed
-- from day one. Produces the complete workspace-scoped PostgreSQL
-- layout used by every `ox-store` trait impl: core tables + RLS
-- policies + indexes + FKs + routing seed rows.
--
-- Organised by concern with thematic section banners. The file is
-- the only migration — sqlx's `_sqlx_migrations` ledger records one
-- version and nothing else. A deploy that already ran the historical
-- 31-migration chain must be reset for this schema to apply cleanly.
-- ============================================================================


-- ============================================================================
-- Schema-wide helper functions
-- ============================================================================
--
-- Defined before any table that references them from a CHECK clause.
-- `fn_validate_locale_chain` backs the
-- `workspaces.admin_locale_fallback` and
-- `workspaces.llm_locale_fallback` constraints: each chain must be
-- a non-empty JSON array of BCP 47 tags (`ko`, `en-us`,
-- `zh-hant-tw`, …).

CREATE OR REPLACE FUNCTION fn_validate_locale_chain(chain jsonb) RETURNS boolean AS $$
DECLARE
    elem jsonb;
BEGIN
    IF jsonb_typeof(chain) <> 'array' THEN
        RETURN false;
    END IF;
    IF jsonb_array_length(chain) = 0 THEN
        RETURN false;
    END IF;
    FOR elem IN SELECT value FROM jsonb_array_elements(chain) LOOP
        IF jsonb_typeof(elem) <> 'string' THEN
            RETURN false;
        END IF;
        IF NOT (elem #>> '{}' ~ '^[a-z]{2,3}(-[a-z0-9]{2,8})*$') THEN
            RETURN false;
        END IF;
    END LOOP;
    RETURN true;
END;
$$ LANGUAGE plpgsql IMMUTABLE;


-- ============================================================================
-- Core tables
-- ============================================================================

CREATE TABLE users (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    email text NOT NULL,
    name text,
    picture text,
    provider text NOT NULL,
    provider_sub text NOT NULL,
    role text DEFAULT 'designer' NOT NULL,
    -- Bulk JWT invalidation counter. Incremented when the entire fleet
    -- of issued tokens for this user must stop being honoured at once
    -- (role downgrade, suspected credential theft, password reset).
    -- Every issued JWT carries the value as the `tv` claim; require_auth
    -- compares the claim against the current row and rejects on
    -- mismatch — no per-token enumeration needed.
    token_version BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    last_login_at TIMESTAMPTZ,
    CONSTRAINT users_pkey PRIMARY KEY (id),
    CONSTRAINT users_email_key UNIQUE (email),
    CONSTRAINT users_provider_provider_sub_key UNIQUE (provider, provider_sub)
);

-- Per-token JWT revocation list. Pairs with users.token_version for
-- two complementary invalidation surfaces:
--
-- - revoked_jwts: explicit per-token revoke (logout, security incident
--   targeting a single session). Keyed by `jti` (UUID-v4 generated at
--   token creation).
-- - users.token_version: bulk invalidation across every token a user
--   ever held. Cheaper than enumerating all issued jtis.
--
-- `expires_at` mirrors the original JWT `exp` so the cleanup cron can
-- delete rows for tokens that have already expired naturally — keeping
-- the table bounded without losing security guarantees (the JWT itself
-- is unusable once `exp` has passed).
--
-- No RLS on this table: JWT revocation is global and queried before
-- workspace context is established. Reads are limited to a single
-- `find_revoked_jwt(jti)` lookup; writes go through the auth handler
-- with a SYSTEM_BYPASS scope.
CREATE TABLE revoked_jwts (
    jti uuid NOT NULL,
    revoked_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_by_user_id uuid,
    reason text,
    CONSTRAINT revoked_jwts_pkey PRIMARY KEY (jti),
    CONSTRAINT revoked_jwts_revoked_by_fkey
        FOREIGN KEY (revoked_by_user_id) REFERENCES users(id) ON DELETE SET NULL
);

CREATE INDEX revoked_jwts_expires_at_idx ON revoked_jwts (expires_at);

-- Idempotency-Key middleware records (ADR-0047). Caller-supplied
-- key on POST/PATCH/PUT/DELETE replays the cached response when
-- the request body hash matches the original; mismatched hash on a
-- reused key surfaces as 409. The dominant cost driver this defends
-- against is LLM-driven endpoints (design / refine / extend / edit
-- / chat) where retries on transient failure double-charge tokens.
--
-- Scope is `(workspace_id, user_id, method, path, key)` so two
-- different routes can reuse the same key without colliding (Stripe
-- pattern). Bodies and response payloads are stored as `bytea`
-- because raw JWT-bearing requests already cap at the API gateway
-- limit and we want to cache exactly what the client sent / received,
-- not a re-serialised reconstruction.
--
-- No RLS: writes are gated by the middleware passing `workspace_id`
-- /  `user_id` from the authenticated principal; the table never
-- backs reads through a workspace-scoped query.
CREATE TABLE idempotency_records (
    workspace_id uuid NOT NULL,
    user_id uuid NOT NULL,
    method text NOT NULL,
    path text NOT NULL,
    key text NOT NULL,
    request_hash bytea NOT NULL,
    response_status smallint NOT NULL,
    response_body bytea NOT NULL,
    response_content_type text,
    created_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    CONSTRAINT idempotency_records_pkey
        PRIMARY KEY (workspace_id, user_id, method, path, key),
    CONSTRAINT idempotency_records_user_fkey
        FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);

CREATE INDEX idempotency_records_expires_at_idx
    ON idempotency_records (expires_at);

CREATE TABLE workspaces (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    name VARCHAR(255) NOT NULL,
    slug VARCHAR(100) NOT NULL,
    owner_id uuid NOT NULL,
    settings jsonb DEFAULT '{}' NOT NULL,
    created_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    -- I18n policy. Two locale chains, one per surface:
    --
    -- * `admin_locale_fallback` — the chain admin / operator UI
    --   walks when picking a translation (Korean primary).
    -- * `llm_locale_fallback` — the chain agent / Brain prompts
    --   and tool-result contexts walk (English primary; LLMs
    --   reason best on English content).
    --
    -- Splitting the chains lets the workspace serve a Korean admin
    -- audience without forcing every LLM tool call to receive
    -- Korean glossary entries first — the platform's quality
    -- signal improves measurably when the model sees its preferred
    -- language for ontology context. `primary_locale` remains the
    -- workspace default for content authoring.
    --
    -- DEFAULTs mirror `ox_core::{PRIMARY_LOCALE_DEFAULT,
    -- ADMIN_LOCALE_FALLBACK_DEFAULT, LLM_LOCALE_FALLBACK_DEFAULT}`
    -- — the contract is pinned by
    -- `i18n::tests::locale_defaults_match_db_column_defaults`.
    --
    -- Both chains stay in BCP 47 shape (e.g. `ko`, `en`, `en-us`,
    -- `zh-hant-tw`) — validated by `fn_validate_locale_chain`,
    -- defined alongside these constraints.
    primary_locale TEXT NOT NULL DEFAULT 'ko',
    admin_locale_fallback JSONB NOT NULL DEFAULT '["ko","en"]'::jsonb,
    llm_locale_fallback JSONB NOT NULL DEFAULT '["en","ko"]'::jsonb,
    CONSTRAINT workspaces_pkey PRIMARY KEY (id),
    CONSTRAINT workspaces_slug_key UNIQUE (slug),
    CONSTRAINT workspaces_primary_locale_check
        CHECK (primary_locale ~ '^[a-z]{2,3}(-[a-z0-9]{2,8})*$'),
    CONSTRAINT workspaces_admin_locale_fallback_check
        CHECK (fn_validate_locale_chain(admin_locale_fallback)),
    CONSTRAINT workspaces_llm_locale_fallback_check
        CHECK (fn_validate_locale_chain(llm_locale_fallback))
);

CREATE TABLE workspace_members (
    workspace_id uuid NOT NULL,
    user_id uuid NOT NULL,
    role VARCHAR(20) DEFAULT 'member' NOT NULL,
    joined_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    CONSTRAINT workspace_members_pkey PRIMARY KEY (workspace_id, user_id),
    CONSTRAINT valid_workspace_role CHECK (role::text = ANY (ARRAY['owner', 'admin', 'member', 'viewer']))
);

-- ============================================================================
-- 2. Design
-- ============================================================================

-- ADR 0011 — declarative bridge between a source schema snapshot
-- and the OntologyIR derived from it. Content-addressed via the
-- artifact body's SHA-256 plus the snapshot hash, so a re-run of
-- the design action against an unchanged schema collapses to the
-- existing row instead of writing a duplicate.
CREATE TABLE source_mapping_artifacts (
    id text NOT NULL,
    source_id text NOT NULL,
    schema_snapshot_hash text NOT NULL,
    content_hash text NOT NULL,
    body jsonb NOT NULL,
    created_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    created_by text NOT NULL,
    workspace_id uuid DEFAULT (current_setting('app.workspace_id', true))::uuid NOT NULL,
    CONSTRAINT source_mapping_artifacts_pkey PRIMARY KEY (id),
    CONSTRAINT source_mapping_artifacts_content_addressed
        UNIQUE (workspace_id, source_id, schema_snapshot_hash, content_hash)
);
ALTER TABLE source_mapping_artifacts FORCE ROW LEVEL SECURITY;
CREATE INDEX idx_source_mapping_artifacts_source
    ON source_mapping_artifacts (workspace_id, source_id, created_at DESC);

CREATE TABLE ontology_drafts (
    id uuid NOT NULL,
    user_id text NOT NULL,
    status text DEFAULT 'analyzed' NOT NULL,
    revision integer DEFAULT 1 NOT NULL,
    title text,
    source_config jsonb NOT NULL,
    source_data text,
    source_schema jsonb,
    source_profile jsonb,
    source_history jsonb DEFAULT '[]' NOT NULL,
    analysis_report jsonb,
    design_options jsonb DEFAULT '{}' NOT NULL,
    -- The `AnalyzeSelection` chosen at project creation. Captures the
    -- exact subset of source tables the operator picked at the
    -- bootstrap step ("all" / "subset(...)" / "extend(...)") so the
    -- decision survives across sessions instead of living only in
    -- the browser's `BOOTSTRAP_STORAGE_KEY` localStorage entry.
    initial_selection jsonb,
    ontology jsonb,
    quality_report jsonb,
    ontology_id uuid,
    -- `{source_type}:{source_fingerprint}` derived from `source_config`.
    -- Federation plan-cache looks the project up by this string so FKs
    -- are not enough; insert paths MUST supply it explicitly.
    source_id TEXT NOT NULL,
    created_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    updated_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    analyzed_at TIMESTAMPTZ,
    archived_at TIMESTAMPTZ,
    workspace_id uuid DEFAULT (current_setting('app.workspace_id', true))::uuid NOT NULL,
    CONSTRAINT ontology_drafts_pkey PRIMARY KEY (id)
);
ALTER TABLE ONLY ontology_drafts FORCE ROW LEVEL SECURITY;

CREATE TABLE ontology_snapshots (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    ontology_draft_id uuid NOT NULL,
    revision integer NOT NULL,
    ontology jsonb NOT NULL,
    quality_report jsonb,
    created_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    workspace_id uuid DEFAULT (current_setting('app.workspace_id', true))::uuid NOT NULL,
    CONSTRAINT ontology_snapshots_pkey PRIMARY KEY (id),
    CONSTRAINT ontology_snapshots_draft_revision_key UNIQUE (ontology_draft_id, revision)
);
ALTER TABLE ONLY ontology_snapshots FORCE ROW LEVEL SECURITY;

-- ============================================================================
-- 3. Query
-- ============================================================================

CREATE TABLE query_executions (
    id uuid NOT NULL,
    user_id text NOT NULL,
    question text NOT NULL,
    ontology_lineage_id text NOT NULL,
    ontology_version integer NOT NULL,
    ontology_id uuid,
    ontology_snapshot jsonb,
    query_ir jsonb NOT NULL,
    compiled_target text NOT NULL,
    compiled_query text NOT NULL,
    results jsonb NOT NULL,
    widget jsonb,
    explanation text NOT NULL,
    model text NOT NULL,
    execution_time_ms bigint NOT NULL,
    query_bindings jsonb,
    feedback VARCHAR(10),
    created_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    workspace_id uuid DEFAULT (current_setting('app.workspace_id', true))::uuid NOT NULL,
    CONSTRAINT query_executions_pkey PRIMARY KEY (id),
    CONSTRAINT chk_ontology_source CHECK ((ontology_id IS NOT NULL) OR (ontology_snapshot IS NOT NULL))
);
ALTER TABLE ONLY query_executions FORCE ROW LEVEL SECURITY;

CREATE TABLE pinboard_items (
    id uuid NOT NULL,
    query_execution_id uuid NOT NULL,
    user_id text NOT NULL,
    widget_spec jsonb NOT NULL,
    title text,
    pinned_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    workspace_id uuid DEFAULT (current_setting('app.workspace_id', true))::uuid NOT NULL,
    CONSTRAINT pinboard_items_pkey PRIMARY KEY (id),
    CONSTRAINT pinboard_items_query_execution_id_key UNIQUE (query_execution_id)
);
ALTER TABLE ONLY pinboard_items FORCE ROW LEVEL SECURITY;

-- Persisted insights — saved multi-hop discoveries with the
-- `QueryIR` re-run anchor + the original ontology/registry
-- provenance. See `ox_query_ir::insight::InsightDef` for the typed
-- shape; `query_ir` and `original_provenance` are JSONB blobs of the
-- canonical wire form.
CREATE TABLE insights (
    id                    TEXT PRIMARY KEY,
    question              JSONB NOT NULL,                 -- LocalizedText
    description           JSONB NOT NULL DEFAULT '{}'::jsonb,
    tags                  TEXT[] NOT NULL DEFAULT '{}',
    -- GlossaryTermId references — typed concept anchors per the
    -- 1-pager's "용어 사전이 다리" axis. Distinct from `tags`
    -- (freeform admin shorthand) so cross-team filtering by concept
    -- stays consistent even when tag wording drifts.
    concept_anchors       TEXT[] NOT NULL DEFAULT '{}',
    query_ir              JSONB NOT NULL,
    original_provenance   JSONB,
    author_id             UUID NOT NULL,
    expires_at            TIMESTAMPTZ,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    workspace_id          UUID NOT NULL DEFAULT (current_setting('app.workspace_id', true))::uuid
                               REFERENCES workspaces(id) ON DELETE CASCADE
);
ALTER TABLE insights ENABLE ROW LEVEL SECURITY;
ALTER TABLE insights FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON insights
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON insights
    USING (current_setting('app.system_bypass', true) = 'true');

-- Filter by author (admin "my insights" tab) + by expiry sweep
-- (background job that hides stale insights from the default
-- surface).
CREATE INDEX insights_author_idx
    ON insights (workspace_id, author_id);
CREATE INDEX insights_expires_idx
    ON insights (workspace_id, expires_at)
    WHERE expires_at IS NOT NULL;
-- Concept-anchor / tag overlap (`array && array`) — pairs with the
-- `InsightFilter` axes (1-pager: "용어 사전이 다리"). GIN is the
-- index family Postgres ships for `&&` over `text[]`.
CREATE INDEX insights_concept_anchors_gin
    ON insights USING gin (concept_anchors);
CREATE INDEX insights_tags_gin
    ON insights USING gin (tags);

-- ============================================================================
-- 4. Agent
-- ============================================================================

CREATE TABLE agent_sessions (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    ontology_lineage_id VARCHAR(255),
    prompt_hash VARCHAR(64) NOT NULL,
    tool_schema_hash VARCHAR(64) NOT NULL,
    model_id VARCHAR(255) NOT NULL,
    model_config jsonb DEFAULT '{}' NOT NULL,
    user_message text NOT NULL,
    final_text text,
    created_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    completed_at TIMESTAMPTZ,
    workspace_id uuid DEFAULT (current_setting('app.workspace_id', true))::uuid NOT NULL,
    CONSTRAINT agent_sessions_pkey PRIMARY KEY (id)
);
ALTER TABLE ONLY agent_sessions FORCE ROW LEVEL SECURITY;

CREATE TABLE agent_events (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    session_id uuid NOT NULL,
    sequence integer NOT NULL,
    event_type VARCHAR(50) NOT NULL,
    payload jsonb NOT NULL,
    created_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    workspace_id uuid DEFAULT (current_setting('app.workspace_id', true))::uuid NOT NULL,
    CONSTRAINT agent_events_pkey PRIMARY KEY (id),
    CONSTRAINT agent_events_session_id_sequence_key UNIQUE (session_id, sequence)
);
ALTER TABLE ONLY agent_events FORCE ROW LEVEL SECURITY;

CREATE TABLE tool_approvals (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    session_id uuid NOT NULL,
    tool_call_id VARCHAR(255) NOT NULL,
    approved boolean NOT NULL,
    reason text,
    modified_input jsonb,
    user_id VARCHAR(255) NOT NULL,
    created_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    workspace_id uuid DEFAULT (current_setting('app.workspace_id', true))::uuid NOT NULL,
    CONSTRAINT tool_approvals_pkey PRIMARY KEY (id),
    CONSTRAINT tool_approvals_session_id_tool_call_id_key UNIQUE (session_id, tool_call_id)
);
ALTER TABLE ONLY tool_approvals FORCE ROW LEVEL SECURITY;

-- ============================================================================
-- 5. Dashboard
-- ============================================================================

CREATE TABLE dashboards (
    id uuid NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    name VARCHAR(255) NOT NULL,
    description text,
    layout jsonb DEFAULT '[]' NOT NULL,
    is_public boolean DEFAULT false NOT NULL,
    created_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    updated_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    workspace_id uuid DEFAULT (current_setting('app.workspace_id', true))::uuid NOT NULL,
    share_token VARCHAR(64),
    shared_at TIMESTAMPTZ,
    -- Expiry for public share tokens; NULL means no expiry.
    share_expires_at TIMESTAMPTZ,
    CONSTRAINT dashboards_pkey PRIMARY KEY (id),
    CONSTRAINT dashboards_share_token_key UNIQUE (share_token)
);
ALTER TABLE ONLY dashboards FORCE ROW LEVEL SECURITY;

CREATE TABLE dashboard_widgets (
    id uuid NOT NULL,
    dashboard_id uuid NOT NULL,
    title VARCHAR(255) NOT NULL,
    widget_type VARCHAR(50) NOT NULL,
    query text,
    widget_spec jsonb DEFAULT '{}' NOT NULL,
    "position" jsonb DEFAULT '{"h": 4, "w": 6, "x": 0, "y": 0}' NOT NULL,
    refresh_interval_secs integer,
    thresholds jsonb,
    last_result jsonb,
    last_refreshed TIMESTAMPTZ,
    created_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    workspace_id uuid DEFAULT (current_setting('app.workspace_id', true))::uuid NOT NULL,
    CONSTRAINT dashboard_widgets_pkey PRIMARY KEY (id)
);
ALTER TABLE ONLY dashboard_widgets FORCE ROW LEVEL SECURITY;

-- ============================================================================
-- 6. Data
-- ============================================================================

CREATE TABLE data_lineage (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    workspace_id uuid DEFAULT (current_setting('app.workspace_id', true))::uuid NOT NULL,
    ontology_draft_id uuid,
    graph_label text NOT NULL,
    graph_element_type text NOT NULL,
    source_type text NOT NULL,
    source_name text NOT NULL,
    source_table text,
    source_columns text[],
    load_plan_hash text,
    record_count bigint DEFAULT 0 NOT NULL,
    loaded_by uuid,
    started_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    completed_at TIMESTAMPTZ,
    status text DEFAULT 'running' NOT NULL,
    error_message text,
    property_mappings jsonb,
    CONSTRAINT data_lineage_pkey PRIMARY KEY (id),
    CONSTRAINT data_lineage_graph_element_type_check CHECK (graph_element_type = ANY (ARRAY['node', 'edge'])),
    CONSTRAINT data_lineage_status_check CHECK (status = ANY (ARRAY['running', 'completed', 'failed']))
);
ALTER TABLE ONLY data_lineage FORCE ROW LEVEL SECURITY;

CREATE TABLE load_checkpoints (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    workspace_id uuid NOT NULL,
    ontology_draft_id uuid NOT NULL,
    source_table VARCHAR(255) NOT NULL,
    graph_label VARCHAR(255) NOT NULL,
    watermark_column VARCHAR(255) NOT NULL,
    watermark_value text NOT NULL,
    record_count bigint DEFAULT 0 NOT NULL,
    loaded_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    CONSTRAINT load_checkpoints_pkey PRIMARY KEY (id),
    CONSTRAINT load_checkpoints_ws_draft_table_label_key UNIQUE (workspace_id, ontology_draft_id, source_table, graph_label)
);

-- ============================================================================
-- 7. Analysis
-- ============================================================================

CREATE TABLE analysis_recipes (
    id uuid NOT NULL,
    name VARCHAR(255) NOT NULL,
    description text NOT NULL,
    algorithm_type VARCHAR(50) NOT NULL,
    code_template text NOT NULL,
    parameters jsonb DEFAULT '[]' NOT NULL,
    required_columns jsonb DEFAULT '[]' NOT NULL,
    output_description text DEFAULT '' NOT NULL,
    created_by VARCHAR(255) NOT NULL,
    created_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    version integer DEFAULT 1 NOT NULL,
    status VARCHAR(20) DEFAULT 'approved' NOT NULL,
    parent_id uuid,
    workspace_id uuid DEFAULT (current_setting('app.workspace_id', true))::uuid NOT NULL,
    CONSTRAINT analysis_recipes_pkey PRIMARY KEY (id)
);
ALTER TABLE ONLY analysis_recipes FORCE ROW LEVEL SECURITY;

CREATE TABLE analysis_results (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    recipe_id uuid,
    ontology_lineage_id VARCHAR(255),
    input_hash VARCHAR(64) NOT NULL,
    output jsonb NOT NULL,
    duration_ms bigint NOT NULL,
    created_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    workspace_id uuid DEFAULT (current_setting('app.workspace_id', true))::uuid NOT NULL,
    CONSTRAINT analysis_results_pkey PRIMARY KEY (id)
);
ALTER TABLE ONLY analysis_results FORCE ROW LEVEL SECURITY;

CREATE TABLE scheduled_tasks (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    recipe_id uuid NOT NULL,
    ontology_lineage_id VARCHAR(255),
    cron_expression VARCHAR(100) NOT NULL,
    description text,
    enabled boolean DEFAULT true NOT NULL,
    last_run_at TIMESTAMPTZ,
    next_run_at TIMESTAMPTZ NOT NULL,
    last_status VARCHAR(20),
    webhook_url text,
    created_by VARCHAR(255) NOT NULL,
    created_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    workspace_id uuid DEFAULT (current_setting('app.workspace_id', true))::uuid NOT NULL,
    CONSTRAINT scheduled_tasks_pkey PRIMARY KEY (id)
);
ALTER TABLE ONLY scheduled_tasks FORCE ROW LEVEL SECURITY;

-- ============================================================================
-- 8. Quality
-- ============================================================================

CREATE TABLE quality_rules (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    workspace_id uuid DEFAULT (current_setting('app.workspace_id', true))::uuid NOT NULL,
    name text NOT NULL,
    description text,
    rule_type text NOT NULL,
    target_label text NOT NULL,
    target_property text,
    threshold FLOAT8 DEFAULT 95.0 NOT NULL,
    cypher_check text,
    severity text DEFAULT 'warning' NOT NULL,
    is_active boolean DEFAULT true NOT NULL,
    created_by uuid,
    created_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    updated_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    ontology_lineage_id text NOT NULL,
    CONSTRAINT quality_rules_pkey PRIMARY KEY (id),
    CONSTRAINT quality_rules_rule_type_check CHECK (rule_type = ANY (ARRAY['completeness', 'uniqueness', 'freshness', 'consistency', 'custom'])),
    CONSTRAINT quality_rules_severity_check CHECK (severity = ANY (ARRAY['critical', 'warning', 'info']))
);
ALTER TABLE ONLY quality_rules FORCE ROW LEVEL SECURITY;

CREATE TABLE quality_results (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    workspace_id uuid DEFAULT (current_setting('app.workspace_id', true))::uuid NOT NULL,
    rule_id uuid NOT NULL,
    passed boolean NOT NULL,
    actual_value numeric(10,4),
    details jsonb DEFAULT '{}',
    evaluated_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    CONSTRAINT quality_results_pkey PRIMARY KEY (id)
);
ALTER TABLE ONLY quality_results FORCE ROW LEVEL SECURITY;

-- ============================================================================
-- 9. Knowledge
-- ============================================================================

CREATE TABLE knowledge_entries (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    workspace_id uuid DEFAULT (current_setting('app.workspace_id', true))::uuid NOT NULL,
    ontology_name VARCHAR(255) NOT NULL,
    ontology_version_min integer DEFAULT 1 NOT NULL,
    ontology_version_max integer,
    kind VARCHAR(50) NOT NULL,
    status VARCHAR(20) DEFAULT 'draft' NOT NULL,
    confidence FLOAT8 DEFAULT 0.5 NOT NULL,
    title VARCHAR(500) NOT NULL,
    content text NOT NULL,
    structured_data jsonb DEFAULT '{}' NOT NULL,
    embedding vector(1024),
    version_checked integer DEFAULT 1 NOT NULL,
    content_hash VARCHAR(64) NOT NULL,
    source_execution_ids uuid[] DEFAULT '{}' NOT NULL,
    source_session_id uuid,
    affected_labels text[] DEFAULT '{}' NOT NULL,
    created_by VARCHAR(255) NOT NULL,
    reviewed_by uuid,
    reviewed_at TIMESTAMPTZ,
    review_notes text,
    use_count bigint DEFAULT 0 NOT NULL,
    last_used_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    updated_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    affected_properties text[] DEFAULT '{}' NOT NULL,
    CONSTRAINT knowledge_entries_pkey PRIMARY KEY (id),
    CONSTRAINT knowledge_entries_confidence_check CHECK ((confidence >= 0.0) AND (confidence <= 1.0))
);
ALTER TABLE ONLY knowledge_entries FORCE ROW LEVEL SECURITY;

CREATE TABLE pending_embeddings (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    content text NOT NULL,
    metadata jsonb NOT NULL,
    retry_count integer DEFAULT 0 NOT NULL,
    last_error text,
    created_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    CONSTRAINT pending_embeddings_pkey PRIMARY KEY (id)
);

-- ============================================================================
-- 10. Memory
-- ============================================================================

CREATE TABLE memory_entries (
    id VARCHAR(255) NOT NULL,
    embedding vector(1024),
    content text NOT NULL,
    metadata jsonb DEFAULT '{}' NOT NULL,
    model_id VARCHAR(100) DEFAULT 'qwen3-0.6b' NOT NULL,
    last_accessed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    workspace_id uuid DEFAULT (current_setting('app.workspace_id', true))::uuid NOT NULL,
    CONSTRAINT memory_entries_pkey PRIMARY KEY (id)
);
ALTER TABLE ONLY memory_entries FORCE ROW LEVEL SECURITY;

-- ============================================================================
-- 11. Notifications
-- ============================================================================

CREATE TABLE notification_channels (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    workspace_id uuid NOT NULL,
    name VARCHAR(255) NOT NULL,
    channel_type VARCHAR(50) NOT NULL,
    config jsonb DEFAULT '{}' NOT NULL,
    events text[] DEFAULT '{}' NOT NULL,
    enabled boolean DEFAULT true NOT NULL,
    created_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    updated_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    CONSTRAINT notification_channels_pkey PRIMARY KEY (id)
);

CREATE TABLE notification_log (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    workspace_id uuid NOT NULL,
    channel_id uuid NOT NULL,
    event_type VARCHAR(100) NOT NULL,
    subject VARCHAR(500) NOT NULL,
    body text NOT NULL,
    status VARCHAR(20) DEFAULT 'pending' NOT NULL,
    error text,
    created_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    CONSTRAINT notification_log_pkey PRIMARY KEY (id)
);

-- ============================================================================
-- 12. Settings
-- ============================================================================

CREATE TABLE model_configs (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    workspace_id uuid,
    name text NOT NULL,
    provider text NOT NULL,
    model_id text NOT NULL,
    max_tokens integer DEFAULT 8192 NOT NULL,
    temperature real,
    timeout_secs integer DEFAULT 300 NOT NULL,
    cost_per_1m_input FLOAT8,
    cost_per_1m_output FLOAT8,
    daily_budget_usd FLOAT8,
    priority integer DEFAULT 0 NOT NULL,
    enabled boolean DEFAULT true NOT NULL,
    api_key_env text,
    region text,
    base_url text,
    provider_meta jsonb DEFAULT '{}' NOT NULL,
    created_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    updated_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    CONSTRAINT model_configs_pkey PRIMARY KEY (id)
);
ALTER TABLE ONLY model_configs FORCE ROW LEVEL SECURITY;

CREATE TABLE model_routing_rules (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    workspace_id uuid,
    operation text NOT NULL,
    model_config_id uuid NOT NULL,
    priority integer DEFAULT 0 NOT NULL,
    enabled boolean DEFAULT true NOT NULL,
    created_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    CONSTRAINT model_routing_rules_pkey PRIMARY KEY (id)
);
ALTER TABLE ONLY model_routing_rules FORCE ROW LEVEL SECURITY;

CREATE TABLE prompt_templates (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    name VARCHAR(100) NOT NULL,
    version VARCHAR(20) NOT NULL,
    content text NOT NULL,
    variables jsonb DEFAULT '[]' NOT NULL,
    metadata jsonb DEFAULT '{}' NOT NULL,
    created_by VARCHAR(255) DEFAULT 'system' NOT NULL,
    created_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    is_active boolean DEFAULT true NOT NULL,
    workspace_id uuid DEFAULT (current_setting('app.workspace_id', true))::uuid,
    CONSTRAINT prompt_templates_pkey PRIMARY KEY (id),
    CONSTRAINT prompt_templates_name_version_key UNIQUE (name, version),
    -- Semver contract: version must be `MAJOR.MINOR.PATCH`.
    CONSTRAINT prompt_templates_version_semver_chk
        CHECK (version ~ '^[0-9]+\.[0-9]+\.[0-9]+$')
);
ALTER TABLE ONLY prompt_templates FORCE ROW LEVEL SECURITY;

CREATE TABLE system_config (
    category text NOT NULL,
    key text NOT NULL,
    value text NOT NULL,
    data_type text DEFAULT 'string' NOT NULL,
    description text DEFAULT '' NOT NULL,
    updated_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    CONSTRAINT system_config_pkey PRIMARY KEY (category, key)
);

-- ============================================================================
-- 13. Governance
-- ============================================================================

CREATE TABLE acl_policies (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    workspace_id uuid DEFAULT (current_setting('app.workspace_id', true))::uuid NOT NULL,
    name text NOT NULL,
    description text,
    subject_type text NOT NULL,
    subject_value text NOT NULL,
    resource_type text NOT NULL,
    resource_value text,
    action text NOT NULL,
    properties text[],
    mask_pattern text DEFAULT '***',
    priority integer DEFAULT 0 NOT NULL,
    is_active boolean DEFAULT true NOT NULL,
    created_by uuid,
    created_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    updated_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    CONSTRAINT acl_policies_pkey PRIMARY KEY (id),
    CONSTRAINT acl_policies_action_check CHECK (action = ANY (ARRAY['mask', 'deny', 'allow'])),
    CONSTRAINT acl_policies_resource_type_check CHECK (resource_type = ANY (ARRAY['node_label', 'edge_label', 'all'])),
    CONSTRAINT acl_policies_subject_type_check CHECK (subject_type = ANY (ARRAY['role', 'user', 'workspace_role']))
);
ALTER TABLE ONLY acl_policies FORCE ROW LEVEL SECURITY;

CREATE TABLE approval_requests (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    workspace_id uuid DEFAULT (current_setting('app.workspace_id', true))::uuid NOT NULL,
    requester_id uuid NOT NULL,
    action_type text NOT NULL,
    resource_type text NOT NULL,
    resource_id text NOT NULL,
    payload jsonb DEFAULT '{}' NOT NULL,
    status text DEFAULT 'pending' NOT NULL,
    reviewer_id uuid,
    reviewed_at TIMESTAMPTZ,
    expires_at TIMESTAMPTZ DEFAULT (now() + '7 days'::interval) NOT NULL,
    created_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    CONSTRAINT approval_requests_pkey PRIMARY KEY (id),
    CONSTRAINT approval_requests_status_check CHECK (status = ANY (ARRAY['pending', 'approved', 'rejected', 'expired']))
);
ALTER TABLE ONLY approval_requests FORCE ROW LEVEL SECURITY;

-- Comment thread attached to an approval. The decision-time rationale
-- the reviewer types on /review lands here as the first entry; any
-- pre- or post-decision discussion follows in the same thread.
CREATE TABLE approval_comments (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    workspace_id uuid DEFAULT (current_setting('app.workspace_id', true))::uuid NOT NULL,
    approval_id uuid NOT NULL,
    author_id uuid NOT NULL,
    body text NOT NULL,
    created_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    CONSTRAINT approval_comments_pkey PRIMARY KEY (id),
    CONSTRAINT approval_comments_body_nonempty CHECK (length(btrim(body)) > 0)
);
ALTER TABLE ONLY approval_comments FORCE ROW LEVEL SECURITY;

CREATE TABLE audit_log (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    user_id uuid,
    workspace_id uuid DEFAULT (current_setting('app.workspace_id', true))::uuid NOT NULL,
    -- When a system task touches a workspace different from the
    -- actor's (cross-workspace admin op), record the affected
    -- workspace here. The `ws_isolation` policy below grants read
    -- access to rows where either workspace_id matches the caller.
    affected_workspace_id UUID,
    action text NOT NULL,
    resource_type text NOT NULL,
    resource_id text,
    details jsonb DEFAULT '{}',
    created_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    CONSTRAINT audit_log_pkey PRIMARY KEY (id)
);
ALTER TABLE ONLY audit_log FORCE ROW LEVEL SECURITY;

CREATE TABLE usage_records (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    workspace_id uuid DEFAULT (current_setting('app.workspace_id', true))::uuid NOT NULL,
    user_id uuid,
    resource_type text NOT NULL,
    provider text,
    model text,
    operation text,
    input_tokens bigint DEFAULT 0,
    output_tokens bigint DEFAULT 0,
    duration_ms bigint DEFAULT 0,
    cost_usd FLOAT8 DEFAULT 0,
    metadata jsonb DEFAULT '{}',
    created_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    CONSTRAINT usage_records_pkey PRIMARY KEY (id)
);
ALTER TABLE ONLY usage_records FORCE ROW LEVEL SECURITY;

CREATE TABLE ontology_verifications (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    ontology_lineage_id VARCHAR(255) NOT NULL,
    element_id VARCHAR(255) NOT NULL,
    element_kind VARCHAR(50) NOT NULL,
    verified_by uuid NOT NULL,
    review_notes text,
    invalidated_at TIMESTAMPTZ,
    invalidation_reason text,
    created_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    workspace_id uuid DEFAULT (current_setting('app.workspace_id', true))::uuid NOT NULL,
    CONSTRAINT ontology_verifications_pkey PRIMARY KEY (id)
);
ALTER TABLE ONLY ontology_verifications FORCE ROW LEVEL SECURITY;

-- ============================================================================
-- 14. UI
-- ============================================================================

CREATE TABLE saved_reports (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    ontology_lineage_id VARCHAR(255) NOT NULL,
    title VARCHAR(255) NOT NULL,
    description text,
    query_template text NOT NULL,
    parameters jsonb DEFAULT '[]' NOT NULL,
    widget_type VARCHAR(50),
    is_public boolean DEFAULT false NOT NULL,
    created_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    updated_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    workspace_id uuid DEFAULT (current_setting('app.workspace_id', true))::uuid NOT NULL,
    CONSTRAINT saved_reports_pkey PRIMARY KEY (id)
);
ALTER TABLE ONLY saved_reports FORCE ROW LEVEL SECURITY;

CREATE TABLE workbench_perspectives (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    user_id text NOT NULL,
    lineage_id text NOT NULL,
    topology_signature text NOT NULL,
    ontology_draft_id uuid,
    name text DEFAULT 'Default' NOT NULL,
    positions jsonb DEFAULT '{}' NOT NULL,
    viewport jsonb DEFAULT '{"x": 0, "y": 0, "zoom": 1}' NOT NULL,
    filters jsonb DEFAULT '{}' NOT NULL,
    collapsed_groups jsonb DEFAULT '[]' NOT NULL,
    is_default boolean DEFAULT false NOT NULL,
    created_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    updated_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    workspace_id uuid DEFAULT (current_setting('app.workspace_id', true))::uuid NOT NULL,
    CONSTRAINT workbench_perspectives_pkey PRIMARY KEY (id),
    CONSTRAINT workbench_perspectives_user_id_lineage_id_name_key UNIQUE (user_id, lineage_id, name)
);
ALTER TABLE ONLY workbench_perspectives FORCE ROW LEVEL SECURITY;

-- ============================================================================
-- Core indexes
-- ============================================================================

CREATE INDEX idx_acl_priority ON acl_policies USING btree (workspace_id, priority DESC);
CREATE INDEX idx_acl_resource ON acl_policies USING btree (resource_type, resource_value);
CREATE INDEX idx_acl_subject ON acl_policies USING btree (subject_type, subject_value);
CREATE INDEX idx_acl_workspace ON acl_policies USING btree (workspace_id, is_active);

-- ============================================================================
-- agent_events
-- ============================================================================
CREATE INDEX idx_agent_events_session ON agent_events USING btree (session_id, sequence);
CREATE INDEX idx_agent_events_ws ON agent_events USING btree (workspace_id, session_id);

-- ============================================================================
-- agent_sessions
-- ============================================================================
CREATE INDEX idx_agent_sessions_completed ON agent_sessions USING btree (completed_at) WHERE (completed_at IS NOT NULL);
CREATE INDEX idx_agent_sessions_user ON agent_sessions USING btree (user_id, created_at DESC);
CREATE INDEX idx_sessions_workspace ON agent_sessions USING btree (workspace_id, created_at DESC);
CREATE UNIQUE INDEX uq_agent_sessions_ws_id ON agent_sessions USING btree (workspace_id, id);

-- ============================================================================
-- analysis_recipes
-- ============================================================================
CREATE INDEX idx_recipes_algorithm_type ON analysis_recipes USING btree (algorithm_type);
CREATE INDEX idx_recipes_parent ON analysis_recipes USING btree (parent_id) WHERE (parent_id IS NOT NULL);
CREATE INDEX idx_recipes_status ON analysis_recipes USING btree (status);
CREATE INDEX idx_recipes_workspace ON analysis_recipes USING btree (workspace_id, created_at DESC);
CREATE UNIQUE INDEX uq_analysis_recipes_ws_id ON analysis_recipes USING btree (workspace_id, id);

-- ============================================================================
-- analysis_results
-- ============================================================================
CREATE INDEX idx_analysis_results_cache ON analysis_results USING btree (input_hash, recipe_id);
CREATE INDEX idx_analysis_results_recipe ON analysis_results USING btree (recipe_id, created_at DESC);
CREATE INDEX idx_analysis_results_recipe_created ON analysis_results USING btree (recipe_id, created_at DESC);
CREATE INDEX idx_results_workspace ON analysis_results USING btree (workspace_id, created_at DESC);

-- ============================================================================
-- approval_requests
-- ============================================================================
CREATE INDEX idx_approval_expires ON approval_requests USING btree (expires_at) WHERE (status = 'pending');
CREATE INDEX idx_approval_requester ON approval_requests USING btree (requester_id, created_at DESC);
CREATE INDEX idx_approval_resource ON approval_requests USING btree (resource_type, resource_id);
CREATE INDEX idx_approval_workspace_status ON approval_requests USING btree (workspace_id, status, created_at DESC);

-- ============================================================================
-- approval_comments
-- ============================================================================
CREATE INDEX idx_approval_comments_thread
    ON approval_comments USING btree (approval_id, created_at);
CREATE INDEX idx_approval_comments_workspace
    ON approval_comments USING btree (workspace_id, created_at DESC);

-- ============================================================================
-- audit_log
-- ============================================================================
CREATE INDEX idx_audit_log_action ON audit_log USING btree (action, created_at DESC);
CREATE INDEX idx_audit_log_resource ON audit_log USING btree (resource_type, resource_id);
CREATE INDEX idx_audit_log_user ON audit_log USING btree (user_id, created_at DESC);
CREATE INDEX idx_audit_log_workspace ON audit_log USING btree (workspace_id, created_at DESC);

-- ============================================================================
-- dashboard_widgets
-- ============================================================================
CREATE INDEX idx_dashboard_widgets_ws ON dashboard_widgets USING btree (workspace_id, dashboard_id);
CREATE INDEX idx_widgets_dashboard ON dashboard_widgets USING btree (dashboard_id);

-- ============================================================================
-- dashboards
-- ============================================================================
CREATE INDEX idx_dashboards_public ON dashboards USING btree (updated_at DESC) WHERE (is_public = true);
CREATE INDEX idx_dashboards_share_token ON dashboards USING btree (share_token) WHERE (share_token IS NOT NULL);
CREATE INDEX idx_dashboards_user ON dashboards USING btree (user_id, updated_at DESC);
CREATE INDEX idx_dashboards_workspace ON dashboards USING btree (workspace_id, updated_at DESC);
CREATE UNIQUE INDEX uq_dashboards_ws_id ON dashboards USING btree (workspace_id, id);

-- ============================================================================
-- data_lineage
-- ============================================================================
CREATE INDEX idx_lineage_label ON data_lineage USING btree (graph_label, graph_element_type);
CREATE INDEX idx_lineage_ontology_draft ON data_lineage USING btree (ontology_draft_id) WHERE (ontology_draft_id IS NOT NULL);
CREATE INDEX idx_lineage_source ON data_lineage USING btree (source_name, source_table);
CREATE INDEX idx_lineage_workspace ON data_lineage USING btree (workspace_id, started_at DESC);

-- ============================================================================
-- ontology_drafts
-- ============================================================================
CREATE INDEX idx_ontology_drafts_status ON ontology_drafts USING btree (status) WHERE (archived_at IS NULL);
CREATE INDEX idx_ontology_drafts_updated_at_id ON ontology_drafts USING btree (updated_at DESC, id DESC) WHERE (archived_at IS NULL);
CREATE INDEX idx_ontology_drafts_user ON ontology_drafts USING btree (user_id, updated_at DESC);
CREATE INDEX idx_projects_archived ON ontology_drafts USING btree (archived_at) WHERE (archived_at IS NOT NULL);
CREATE INDEX idx_projects_workspace ON ontology_drafts USING btree (workspace_id, created_at DESC);
CREATE UNIQUE INDEX uq_ontology_drafts_ws_id ON ontology_drafts USING btree (workspace_id, id);

-- ============================================================================
-- knowledge_entries
-- ============================================================================
CREATE INDEX idx_knowledge_affected_labels ON knowledge_entries USING gin (affected_labels);
CREATE INDEX idx_knowledge_affected_properties ON knowledge_entries USING gin (affected_properties);
CREATE INDEX idx_knowledge_confidence ON knowledge_entries USING btree (workspace_id, confidence DESC) WHERE (status::text = 'approved');
CREATE INDEX idx_knowledge_ontology_active ON knowledge_entries USING btree (workspace_id, ontology_name, status) WHERE (status::text = ANY (ARRAY['approved', 'draft']));
CREATE INDEX idx_knowledge_workspace ON knowledge_entries USING btree (workspace_id);
CREATE UNIQUE INDEX uq_knowledge_content_hash ON knowledge_entries USING btree (workspace_id, ontology_name, content_hash);
CREATE UNIQUE INDEX uq_knowledge_ws_id ON knowledge_entries USING btree (workspace_id, id);

-- ============================================================================
-- memory_entries
-- ============================================================================
CREATE INDEX idx_memory_content_trgm ON memory_entries USING gin (content gin_trgm_ops);
CREATE INDEX idx_memory_embedding ON memory_entries USING hnsw (embedding vector_cosine_ops);
CREATE INDEX idx_memory_last_accessed ON memory_entries USING btree (last_accessed_at) WHERE (last_accessed_at IS NOT NULL);
CREATE INDEX idx_memory_metadata_lineage
    ON memory_entries USING btree ((metadata ->> 'ontology_lineage_id'));
CREATE INDEX idx_memory_source ON memory_entries USING btree ((metadata ->> 'source'));
CREATE INDEX idx_memory_workspace ON memory_entries USING btree (workspace_id);

-- ============================================================================
-- model_configs
-- ============================================================================
CREATE UNIQUE INDEX idx_model_configs_scope_name ON model_configs USING btree (COALESCE(workspace_id, '00000000-0000-0000-0000-000000000000'::uuid), name);

-- ============================================================================
-- model_routing_rules
-- ============================================================================
CREATE INDEX idx_routing_lookup ON model_routing_rules USING btree (COALESCE(workspace_id, '00000000-0000-0000-0000-000000000000'::uuid), operation, priority DESC);

-- ============================================================================
-- notification_channels
-- ============================================================================
CREATE INDEX idx_notification_channels_workspace ON notification_channels USING btree (workspace_id);

-- ============================================================================
-- notification_log
-- ============================================================================
CREATE INDEX idx_notification_log_channel ON notification_log USING btree (channel_id);
CREATE INDEX idx_notification_log_workspace ON notification_log USING btree (workspace_id, created_at DESC);

-- ============================================================================
-- ontology_snapshots
-- ============================================================================
CREATE INDEX idx_ontology_snapshots_draft ON ontology_snapshots USING btree (ontology_draft_id, revision DESC);
CREATE INDEX idx_ontology_snapshots_ws ON ontology_snapshots USING btree (workspace_id, ontology_draft_id);

-- ============================================================================
-- ontology_verifications
-- ============================================================================
CREATE UNIQUE INDEX idx_verifications_active ON ontology_verifications USING btree (ontology_lineage_id, element_id, verified_by) WHERE (invalidated_at IS NULL);
CREATE INDEX idx_verifications_lineage ON ontology_verifications USING btree (ontology_lineage_id) WHERE (invalidated_at IS NULL);
CREATE INDEX idx_verifications_workspace ON ontology_verifications USING btree (workspace_id);

-- ============================================================================
-- pending_embeddings
-- ============================================================================
CREATE INDEX idx_pending_embeddings_retry ON pending_embeddings USING btree (retry_count, created_at) WHERE (retry_count < 3);

-- ============================================================================
-- pinboard_items
-- ============================================================================
CREATE INDEX idx_pinboard_user ON pinboard_items USING btree (user_id, pinned_at DESC, id DESC);
CREATE INDEX idx_pins_workspace ON pinboard_items USING btree (workspace_id);

-- ============================================================================
-- prompt_templates
-- ============================================================================
CREATE INDEX idx_prompt_templates_active ON prompt_templates USING btree (name, is_active) WHERE (is_active = true);
CREATE INDEX idx_templates_workspace ON prompt_templates USING btree (workspace_id);

-- ============================================================================
-- quality_results
-- ============================================================================
CREATE INDEX idx_quality_results_rule ON quality_results USING btree (rule_id, evaluated_at DESC);
CREATE INDEX idx_quality_results_workspace ON quality_results USING btree (workspace_id, evaluated_at DESC);

-- ============================================================================
-- quality_rules
-- ============================================================================
CREATE INDEX idx_quality_rules_active ON quality_rules USING btree (is_active, severity);
CREATE INDEX idx_quality_rules_label ON quality_rules USING btree (target_label);
CREATE INDEX idx_quality_rules_workspace ON quality_rules USING btree (workspace_id);
CREATE UNIQUE INDEX uq_quality_rules_ws_id ON quality_rules USING btree (workspace_id, id);

-- ============================================================================
-- query_executions
-- ============================================================================
CREATE INDEX idx_queries_workspace ON query_executions USING btree (workspace_id, created_at DESC);
CREATE INDEX idx_query_executions_ontology_id
    ON query_executions USING btree (ontology_id)
    WHERE ontology_id IS NOT NULL;
CREATE INDEX idx_query_executions_user ON query_executions USING btree (user_id, created_at DESC);
CREATE UNIQUE INDEX uq_query_executions_ws_id ON query_executions USING btree (workspace_id, id);

-- ============================================================================
-- ============================================================================
-- ============================================================================
-- saved_reports
-- ============================================================================
CREATE INDEX idx_reports_workspace ON saved_reports USING btree (workspace_id, updated_at DESC);
CREATE INDEX idx_saved_reports_lineage ON saved_reports USING btree (ontology_lineage_id);
CREATE INDEX idx_saved_reports_public ON saved_reports USING btree (updated_at DESC) WHERE (is_public = true);
CREATE INDEX idx_saved_reports_user ON saved_reports USING btree (user_id, updated_at DESC);

-- ============================================================================
-- scheduled_tasks
-- ============================================================================
CREATE INDEX idx_schedtasks_workspace ON scheduled_tasks USING btree (workspace_id);
CREATE INDEX idx_scheduled_tasks_next_run ON scheduled_tasks USING btree (next_run_at) WHERE (enabled = true);
CREATE INDEX idx_scheduled_tasks_recipe ON scheduled_tasks USING btree (recipe_id);

-- ============================================================================
-- tool_approvals
-- ============================================================================
CREATE INDEX idx_tool_approvals_session ON tool_approvals USING btree (session_id, created_at DESC);
CREATE INDEX idx_tool_approvals_ws ON tool_approvals USING btree (workspace_id, session_id);

-- ============================================================================
-- usage_records
-- ============================================================================
CREATE INDEX idx_usage_resource_type ON usage_records USING btree (resource_type, created_at DESC);
CREATE INDEX idx_usage_user_time ON usage_records USING btree (user_id, created_at DESC);
CREATE INDEX idx_usage_workspace_time ON usage_records USING btree (workspace_id, created_at DESC);

-- ============================================================================
-- workbench_perspectives
-- ============================================================================
CREATE INDEX idx_perspectives_topology ON workbench_perspectives USING btree (user_id, topology_signature);
CREATE UNIQUE INDEX idx_perspectives_unique_default ON workbench_perspectives USING btree (user_id, lineage_id) WHERE (is_default = true);
CREATE INDEX idx_perspectives_user_lineage ON workbench_perspectives USING btree (user_id, lineage_id);
CREATE INDEX idx_workbench_perspectives_ws ON workbench_perspectives USING btree (workspace_id);

-- ============================================================================
-- workspace_members
-- ============================================================================
CREATE INDEX idx_workspace_members_user ON workspace_members USING btree (user_id);

-- ============================================================================
-- Core foreign keys
-- ============================================================================

ALTER TABLE ONLY workspaces
    ADD CONSTRAINT workspaces_owner_id_fkey FOREIGN KEY (owner_id) REFERENCES users(id);

-- ============================================================================
-- workspace_members
-- ============================================================================
ALTER TABLE ONLY workspace_members
    ADD CONSTRAINT workspace_members_user_id_fkey FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE;
ALTER TABLE ONLY workspace_members
    ADD CONSTRAINT workspace_members_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;

-- ============================================================================
-- ontology_drafts
-- ============================================================================
ALTER TABLE ONLY ontology_drafts
    ADD CONSTRAINT ontology_drafts_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;
-- ============================================================================
-- ontology_snapshots
-- ============================================================================
ALTER TABLE ONLY ontology_snapshots
    ADD CONSTRAINT ontology_snapshots_workspace_fk FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;
ALTER TABLE ONLY ontology_snapshots
    ADD CONSTRAINT ontology_snapshots_draft_ws_fk FOREIGN KEY (workspace_id, ontology_draft_id) REFERENCES ontology_drafts(workspace_id, id) ON DELETE CASCADE;

-- ============================================================================
-- ============================================================================
-- ============================================================================
-- query_executions
-- ============================================================================
ALTER TABLE ONLY query_executions
    ADD CONSTRAINT query_executions_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;
-- ============================================================================
-- pinboard_items
-- ============================================================================
ALTER TABLE ONLY pinboard_items
    ADD CONSTRAINT pinboard_items_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;
ALTER TABLE ONLY pinboard_items
    ADD CONSTRAINT pinboard_items_query_execution_ws_fk FOREIGN KEY (workspace_id, query_execution_id) REFERENCES query_executions(workspace_id, id) ON DELETE CASCADE;

-- ============================================================================
-- agent_sessions
-- ============================================================================
ALTER TABLE ONLY agent_sessions
    ADD CONSTRAINT agent_sessions_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;

-- ============================================================================
-- agent_events
-- ============================================================================
ALTER TABLE ONLY agent_events
    ADD CONSTRAINT agent_events_workspace_fk FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;
ALTER TABLE ONLY agent_events
    ADD CONSTRAINT agent_events_session_ws_fk FOREIGN KEY (workspace_id, session_id) REFERENCES agent_sessions(workspace_id, id) ON DELETE CASCADE;

-- ============================================================================
-- tool_approvals
-- ============================================================================
ALTER TABLE ONLY tool_approvals
    ADD CONSTRAINT tool_approvals_workspace_fk FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;
ALTER TABLE ONLY tool_approvals
    ADD CONSTRAINT tool_approvals_session_ws_fk FOREIGN KEY (workspace_id, session_id) REFERENCES agent_sessions(workspace_id, id) ON DELETE CASCADE;

-- ============================================================================
-- dashboards
-- ============================================================================
ALTER TABLE ONLY dashboards
    ADD CONSTRAINT dashboards_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;

-- ============================================================================
-- dashboard_widgets
-- ============================================================================
ALTER TABLE ONLY dashboard_widgets
    ADD CONSTRAINT dashboard_widgets_workspace_fk FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;
ALTER TABLE ONLY dashboard_widgets
    ADD CONSTRAINT dashboard_widgets_dashboard_ws_fk FOREIGN KEY (workspace_id, dashboard_id) REFERENCES dashboards(workspace_id, id) ON DELETE CASCADE;

-- ============================================================================
-- data_lineage
-- ============================================================================
ALTER TABLE ONLY data_lineage
    ADD CONSTRAINT data_lineage_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;
ALTER TABLE ONLY data_lineage
    ADD CONSTRAINT data_lineage_draft_ws_fk FOREIGN KEY (workspace_id, ontology_draft_id) REFERENCES ontology_drafts(workspace_id, id) ON DELETE SET NULL;
ALTER TABLE ONLY data_lineage
    ADD CONSTRAINT data_lineage_loaded_by_fkey FOREIGN KEY (loaded_by) REFERENCES users(id);

-- ============================================================================
-- analysis_recipes
-- ============================================================================
ALTER TABLE ONLY analysis_recipes
    ADD CONSTRAINT analysis_recipes_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;
ALTER TABLE ONLY analysis_recipes
    ADD CONSTRAINT analysis_recipes_parent_id_fkey FOREIGN KEY (parent_id) REFERENCES analysis_recipes(id) ON DELETE SET NULL;

-- ============================================================================
-- analysis_results
-- ============================================================================
ALTER TABLE ONLY analysis_results
    ADD CONSTRAINT analysis_results_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;
ALTER TABLE ONLY analysis_results
    ADD CONSTRAINT analysis_results_recipe_ws_fk FOREIGN KEY (workspace_id, recipe_id) REFERENCES analysis_recipes(workspace_id, id) ON DELETE CASCADE;

-- ============================================================================
-- scheduled_tasks
-- ============================================================================
ALTER TABLE ONLY scheduled_tasks
    ADD CONSTRAINT scheduled_tasks_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;
ALTER TABLE ONLY scheduled_tasks
    ADD CONSTRAINT scheduled_tasks_recipe_ws_fk FOREIGN KEY (workspace_id, recipe_id) REFERENCES analysis_recipes(workspace_id, id) ON DELETE CASCADE;

-- ============================================================================
-- quality_rules
-- ============================================================================
ALTER TABLE ONLY quality_rules
    ADD CONSTRAINT quality_rules_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;
ALTER TABLE ONLY quality_rules
    ADD CONSTRAINT quality_rules_created_by_fkey FOREIGN KEY (created_by) REFERENCES users(id);

-- ============================================================================
-- quality_results
-- ============================================================================
ALTER TABLE ONLY quality_results
    ADD CONSTRAINT quality_results_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;
ALTER TABLE ONLY quality_results
    ADD CONSTRAINT quality_results_rule_ws_fk FOREIGN KEY (workspace_id, rule_id) REFERENCES quality_rules(workspace_id, id) ON DELETE CASCADE;

-- ============================================================================
-- knowledge_entries
-- ============================================================================
ALTER TABLE ONLY knowledge_entries
    ADD CONSTRAINT knowledge_entries_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;
ALTER TABLE ONLY knowledge_entries
    ADD CONSTRAINT knowledge_entries_reviewed_by_fkey FOREIGN KEY (reviewed_by) REFERENCES users(id) ON DELETE SET NULL;

-- ============================================================================
-- memory_entries
-- ============================================================================
ALTER TABLE ONLY memory_entries
    ADD CONSTRAINT fk_memory_workspace FOREIGN KEY (workspace_id) REFERENCES workspaces(id);
ALTER TABLE ONLY memory_entries
    ADD CONSTRAINT memory_entries_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;

-- ============================================================================
-- model_configs
-- ============================================================================
ALTER TABLE ONLY model_configs
    ADD CONSTRAINT model_configs_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;

-- ============================================================================
-- model_routing_rules
-- ============================================================================
ALTER TABLE ONLY model_routing_rules
    ADD CONSTRAINT model_routing_rules_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;
ALTER TABLE ONLY model_routing_rules
    ADD CONSTRAINT model_routing_rules_model_config_id_fkey FOREIGN KEY (model_config_id) REFERENCES model_configs(id) ON DELETE CASCADE;

-- ============================================================================
-- notification_log
-- ============================================================================
ALTER TABLE ONLY notification_log
    ADD CONSTRAINT notification_log_channel_id_fkey FOREIGN KEY (channel_id) REFERENCES notification_channels(id) ON DELETE CASCADE;

-- ============================================================================
-- prompt_templates
-- ============================================================================
ALTER TABLE ONLY prompt_templates
    ADD CONSTRAINT prompt_templates_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;

-- ============================================================================
-- acl_policies
-- ============================================================================
ALTER TABLE ONLY acl_policies
    ADD CONSTRAINT acl_policies_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;
ALTER TABLE ONLY acl_policies
    ADD CONSTRAINT acl_policies_created_by_fkey FOREIGN KEY (created_by) REFERENCES users(id);

-- ============================================================================
-- approval_requests
-- ============================================================================
ALTER TABLE ONLY approval_requests
    ADD CONSTRAINT approval_requests_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;
ALTER TABLE ONLY approval_requests
    ADD CONSTRAINT approval_requests_requester_id_fkey FOREIGN KEY (requester_id) REFERENCES users(id);
ALTER TABLE ONLY approval_requests
    ADD CONSTRAINT approval_requests_reviewer_id_fkey FOREIGN KEY (reviewer_id) REFERENCES users(id);

-- ============================================================================
-- approval_comments
-- ============================================================================
ALTER TABLE ONLY approval_comments
    ADD CONSTRAINT approval_comments_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;
ALTER TABLE ONLY approval_comments
    ADD CONSTRAINT approval_comments_approval_id_fkey FOREIGN KEY (approval_id) REFERENCES approval_requests(id) ON DELETE CASCADE;
ALTER TABLE ONLY approval_comments
    ADD CONSTRAINT approval_comments_author_id_fkey FOREIGN KEY (author_id) REFERENCES users(id);

-- ============================================================================
-- audit_log
-- ============================================================================
ALTER TABLE ONLY audit_log
    ADD CONSTRAINT audit_log_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;
ALTER TABLE ONLY audit_log
    ADD CONSTRAINT audit_log_user_id_fkey FOREIGN KEY (user_id) REFERENCES users(id);

-- ============================================================================
-- usage_records
-- ============================================================================
ALTER TABLE ONLY usage_records
    ADD CONSTRAINT usage_records_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;
ALTER TABLE ONLY usage_records
    ADD CONSTRAINT usage_records_user_id_fkey FOREIGN KEY (user_id) REFERENCES users(id);

-- ============================================================================
-- ontology_verifications
-- ============================================================================
ALTER TABLE ONLY ontology_verifications
    ADD CONSTRAINT ontology_verifications_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;
ALTER TABLE ONLY ontology_verifications
    ADD CONSTRAINT ontology_verifications_verified_by_fkey FOREIGN KEY (verified_by) REFERENCES users(id) ON DELETE CASCADE;

-- ============================================================================
-- saved_reports
-- ============================================================================
ALTER TABLE ONLY saved_reports
    ADD CONSTRAINT saved_reports_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;

-- ============================================================================
-- workbench_perspectives
-- ============================================================================
ALTER TABLE ONLY workbench_perspectives
    ADD CONSTRAINT workbench_perspectives_workspace_fk FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE CASCADE;
ALTER TABLE ONLY workbench_perspectives
    ADD CONSTRAINT workbench_perspectives_draft_ws_fk FOREIGN KEY (workspace_id, ontology_draft_id) REFERENCES ontology_drafts(workspace_id, id) ON DELETE SET NULL;

-- ============================================================================
-- Row-level security policies
-- ============================================================================

ALTER TABLE acl_policies ENABLE ROW LEVEL SECURITY;
ALTER TABLE acl_policies FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON acl_policies
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON acl_policies
    USING (current_setting('app.system_bypass', true) = 'true');

-- ============================================================================
-- agent_events
-- ============================================================================
ALTER TABLE agent_events ENABLE ROW LEVEL SECURITY;
ALTER TABLE agent_events FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON agent_events
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON agent_events
    USING (current_setting('app.system_bypass', true) = 'true');

-- ============================================================================
-- agent_sessions
-- ============================================================================
ALTER TABLE agent_sessions ENABLE ROW LEVEL SECURITY;
ALTER TABLE agent_sessions FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON agent_sessions
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON agent_sessions
    USING (current_setting('app.system_bypass', true) = 'true');

-- ============================================================================
-- analysis_recipes
-- ============================================================================
ALTER TABLE analysis_recipes ENABLE ROW LEVEL SECURITY;
ALTER TABLE analysis_recipes FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON analysis_recipes
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON analysis_recipes
    USING (current_setting('app.system_bypass', true) = 'true');

-- ============================================================================
-- analysis_results
-- ============================================================================
ALTER TABLE analysis_results ENABLE ROW LEVEL SECURITY;
ALTER TABLE analysis_results FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON analysis_results
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON analysis_results
    USING (current_setting('app.system_bypass', true) = 'true');

-- ============================================================================
-- approval_requests
-- ============================================================================
ALTER TABLE approval_requests ENABLE ROW LEVEL SECURITY;
ALTER TABLE approval_requests FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON approval_requests
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON approval_requests
    USING (current_setting('app.system_bypass', true) = 'true');

-- ============================================================================
-- approval_comments
-- ============================================================================
ALTER TABLE approval_comments ENABLE ROW LEVEL SECURITY;
ALTER TABLE approval_comments FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON approval_comments
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON approval_comments
    USING (current_setting('app.system_bypass', true) = 'true');

-- ============================================================================
-- audit_log
-- ============================================================================
-- NOTE: the `ws_isolation` policy for audit_log is defined alongside
-- the `affected_workspace_id` column further below (section
-- "Governance extensions") so a workspace admin can read rows where
-- their workspace was *affected* by a system task, not just rows
-- whose direct owner they are.
ALTER TABLE audit_log ENABLE ROW LEVEL SECURITY;
ALTER TABLE audit_log FORCE ROW LEVEL SECURITY;
CREATE POLICY system_bypass ON audit_log
    USING (current_setting('app.system_bypass', true) = 'true');

-- ============================================================================
-- dashboard_widgets
-- ============================================================================
ALTER TABLE dashboard_widgets ENABLE ROW LEVEL SECURITY;
ALTER TABLE dashboard_widgets FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON dashboard_widgets
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON dashboard_widgets
    USING (current_setting('app.system_bypass', true) = 'true');

-- ============================================================================
-- dashboards
-- ============================================================================
ALTER TABLE dashboards ENABLE ROW LEVEL SECURITY;
ALTER TABLE dashboards FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON dashboards
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON dashboards
    USING (current_setting('app.system_bypass', true) = 'true');

-- ============================================================================
-- data_lineage
-- ============================================================================
ALTER TABLE data_lineage ENABLE ROW LEVEL SECURITY;
ALTER TABLE data_lineage FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON data_lineage
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON data_lineage
    USING (current_setting('app.system_bypass', true) = 'true');

-- ============================================================================
-- ontology_drafts
-- ============================================================================
ALTER TABLE ontology_drafts ENABLE ROW LEVEL SECURITY;
ALTER TABLE ontology_drafts FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON ontology_drafts
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON ontology_drafts
    USING (current_setting('app.system_bypass', true) = 'true');

-- ============================================================================
-- source_mapping_artifacts
-- ============================================================================
ALTER TABLE source_mapping_artifacts ENABLE ROW LEVEL SECURITY;
ALTER TABLE source_mapping_artifacts FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON source_mapping_artifacts
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON source_mapping_artifacts
    USING (current_setting('app.system_bypass', true) = 'true');

-- ============================================================================
-- knowledge_entries
-- ============================================================================
ALTER TABLE knowledge_entries ENABLE ROW LEVEL SECURITY;
ALTER TABLE knowledge_entries FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON knowledge_entries
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON knowledge_entries
    USING (current_setting('app.system_bypass', true) = 'true');

-- ============================================================================
-- load_checkpoints
-- ============================================================================
ALTER TABLE load_checkpoints ENABLE ROW LEVEL SECURITY;
ALTER TABLE load_checkpoints FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON load_checkpoints
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON load_checkpoints
    USING (current_setting('app.system_bypass', true) = 'true');

-- ============================================================================
-- memory_entries
-- ============================================================================
ALTER TABLE memory_entries ENABLE ROW LEVEL SECURITY;
ALTER TABLE memory_entries FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON memory_entries
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON memory_entries
    USING (current_setting('app.system_bypass', true) = 'true');

-- ============================================================================
-- model_configs (ws_or_global -- workspace_id can be NULL for global configs)
-- ============================================================================
ALTER TABLE model_configs ENABLE ROW LEVEL SECURITY;
ALTER TABLE model_configs FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_or_global ON model_configs
    USING (workspace_id IS NULL OR workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON model_configs
    USING (current_setting('app.system_bypass', true) = 'true');

-- ============================================================================
-- model_routing_rules (ws_or_global -- workspace_id can be NULL for global rules)
-- ============================================================================
ALTER TABLE model_routing_rules ENABLE ROW LEVEL SECURITY;
ALTER TABLE model_routing_rules FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_or_global ON model_routing_rules
    USING (workspace_id IS NULL OR workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON model_routing_rules
    USING (current_setting('app.system_bypass', true) = 'true');

-- ============================================================================
-- notification_channels
-- ============================================================================
ALTER TABLE notification_channels ENABLE ROW LEVEL SECURITY;
ALTER TABLE notification_channels FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON notification_channels
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON notification_channels
    USING (current_setting('app.system_bypass', true) = 'true');

-- ============================================================================
-- notification_log
-- ============================================================================
ALTER TABLE notification_log ENABLE ROW LEVEL SECURITY;
ALTER TABLE notification_log FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON notification_log
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON notification_log
    USING (current_setting('app.system_bypass', true) = 'true');

-- ============================================================================
-- ontology_snapshots
-- ============================================================================
ALTER TABLE ontology_snapshots ENABLE ROW LEVEL SECURITY;
ALTER TABLE ontology_snapshots FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON ontology_snapshots
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON ontology_snapshots
    USING (current_setting('app.system_bypass', true) = 'true');

-- ============================================================================
-- ontology_verifications
-- ============================================================================
ALTER TABLE ontology_verifications ENABLE ROW LEVEL SECURITY;
ALTER TABLE ontology_verifications FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON ontology_verifications
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON ontology_verifications
    USING (current_setting('app.system_bypass', true) = 'true');

-- ============================================================================
-- pinboard_items
-- ============================================================================
ALTER TABLE pinboard_items ENABLE ROW LEVEL SECURITY;
ALTER TABLE pinboard_items FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON pinboard_items
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON pinboard_items
    USING (current_setting('app.system_bypass', true) = 'true');

-- ============================================================================
-- prompt_templates
-- ============================================================================
ALTER TABLE prompt_templates ENABLE ROW LEVEL SECURITY;
ALTER TABLE prompt_templates FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_or_global ON prompt_templates
    USING (workspace_id IS NULL OR workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON prompt_templates
    USING (current_setting('app.system_bypass', true) = 'true');

-- ============================================================================
-- quality_results
-- ============================================================================
ALTER TABLE quality_results ENABLE ROW LEVEL SECURITY;
ALTER TABLE quality_results FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON quality_results
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON quality_results
    USING (current_setting('app.system_bypass', true) = 'true');

-- ============================================================================
-- quality_rules
-- ============================================================================
ALTER TABLE quality_rules ENABLE ROW LEVEL SECURITY;
ALTER TABLE quality_rules FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON quality_rules
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON quality_rules
    USING (current_setting('app.system_bypass', true) = 'true');

-- ============================================================================
-- query_executions
-- ============================================================================
ALTER TABLE query_executions ENABLE ROW LEVEL SECURITY;
ALTER TABLE query_executions FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON query_executions
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON query_executions
    USING (current_setting('app.system_bypass', true) = 'true');

-- ============================================================================
-- ============================================================================
-- ============================================================================
-- saved_reports
-- ============================================================================
ALTER TABLE saved_reports ENABLE ROW LEVEL SECURITY;
ALTER TABLE saved_reports FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON saved_reports
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON saved_reports
    USING (current_setting('app.system_bypass', true) = 'true');

-- ============================================================================
-- scheduled_tasks
-- ============================================================================
ALTER TABLE scheduled_tasks ENABLE ROW LEVEL SECURITY;
ALTER TABLE scheduled_tasks FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON scheduled_tasks
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON scheduled_tasks
    USING (current_setting('app.system_bypass', true) = 'true');

-- ============================================================================
-- tool_approvals
-- ============================================================================
ALTER TABLE tool_approvals ENABLE ROW LEVEL SECURITY;
ALTER TABLE tool_approvals FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON tool_approvals
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON tool_approvals
    USING (current_setting('app.system_bypass', true) = 'true');

-- ============================================================================
-- usage_records
-- ============================================================================
ALTER TABLE usage_records ENABLE ROW LEVEL SECURITY;
ALTER TABLE usage_records FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON usage_records
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON usage_records
    USING (current_setting('app.system_bypass', true) = 'true');

-- ============================================================================
-- workbench_perspectives
-- ============================================================================
ALTER TABLE workbench_perspectives ENABLE ROW LEVEL SECURITY;
ALTER TABLE workbench_perspectives FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON workbench_perspectives
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON workbench_perspectives
    USING (current_setting('app.system_bypass', true) = 'true');

-- ============================================================================
-- Governance extensions — audit_log reach + api_keys + prompt overrides
-- ============================================================================
-- Columns that live on the owner table are declared inline in each
-- CREATE TABLE above (`audit_log.affected_workspace_id`,
-- `dashboards.share_expires_at`, `prompt_templates.workspace_id`,
-- `workspaces.primary_locale` / `admin_locale_fallback` /
-- `llm_locale_fallback`); this section
-- ships only the pieces that can't fit inside a single CREATE —
-- namely the composite index + extended RLS policy + a full sibling
-- table for `api_keys`.

-- audit_log: index on the affected-workspace pointer, plus the
-- extended ws_isolation that lets workspace admins see rows where
-- their workspace was *affected* by a system task as well as rows
-- they own. (`ENABLE ROW LEVEL SECURITY` + `system_bypass` policy
-- land in the core-RLS section above.)
CREATE INDEX idx_audit_affected_ws
    ON audit_log (affected_workspace_id)
    WHERE affected_workspace_id IS NOT NULL;

CREATE POLICY ws_isolation ON audit_log
    USING (
        workspace_id = current_setting('app.workspace_id', true)::uuid
        OR affected_workspace_id = current_setting('app.workspace_id', true)::uuid
    )
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);

-- Prompt templates: per-workspace override via a partial unique
-- index. Global templates (workspace_id IS NULL) remain the
-- fallback and the `prompt_templates_name_version_key` constraint
-- (inline on the CREATE) already enforces global uniqueness.
CREATE UNIQUE INDEX uq_prompt_ws_name_version
    ON prompt_templates (name, version, workspace_id)
    WHERE workspace_id IS NOT NULL;

-- API-key identity tracking — labelled keys with the three-role
-- vocabulary mirrored in `Principal::Role` (see ox-api). A NULL
-- workspace marks a platform-admin global key, usable only under
-- SYSTEM_BYPASS; the auth middleware resolves key hashes under
-- SYSTEM_BYPASS so login still works for those.
CREATE TABLE api_keys (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    label TEXT NOT NULL,
    key_hash BYTEA NOT NULL,
    created_by TEXT NOT NULL,
    workspace_id UUID,
    role TEXT NOT NULL DEFAULT 'viewer',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    revoked_at TIMESTAMPTZ,
    CONSTRAINT api_keys_role_check CHECK (role IN ('admin', 'designer', 'viewer'))
);
CREATE INDEX idx_api_keys_hash ON api_keys (key_hash);

ALTER TABLE api_keys ENABLE ROW LEVEL SECURITY;
ALTER TABLE api_keys FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON api_keys
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON api_keys
    USING (current_setting('app.system_bypass', true) = 'true');


-- ============================================================================
-- Saved query patterns
-- ============================================================================

CREATE TABLE saved_query_patterns (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    user_id text NOT NULL,
    ontology_lineage_id text NOT NULL,
    name text NOT NULL,
    description text,
    pattern_ir jsonb NOT NULL,
    created_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    updated_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    workspace_id uuid DEFAULT (current_setting('app.workspace_id', true))::uuid NOT NULL,
    CONSTRAINT saved_query_patterns_pkey PRIMARY KEY (id),
    CONSTRAINT saved_query_patterns_user_ontology_name_key
        UNIQUE (user_id, ontology_lineage_id, name)
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
CREATE INDEX idx_saved_query_patterns_user_lineage
    ON saved_query_patterns (user_id, ontology_lineage_id, updated_at DESC);

-- ============================================================================
-- Data sources (federation adapter registry)
-- ============================================================================

CREATE TABLE data_sources (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    -- Defaults to the task-local `app.workspace_id` the HTTP middleware
    -- sets on the connection. Inserts therefore do not have to name
    -- workspace_id explicitly — the session setting carries it. Every
    -- workspace-scoped table in this schema uses the same default.
    workspace_id  UUID NOT NULL DEFAULT (current_setting('app.workspace_id', true))::uuid
                       REFERENCES workspaces(id) ON DELETE CASCADE,
    source_id     TEXT NOT NULL,
    kind          TEXT NOT NULL,
    config        JSONB NOT NULL,

    -- Last full or partial AnalysisResult cached for this source.
    -- NULL until the first analyze_* call lands. JSONB lets the
    -- shape evolve without a follow-up migration; the canonical
    -- producer is `ox_source::AnalysisResult` (schema + profile +
    -- warnings). Subsetted analyses are still cached here — the
    -- selection used to produce them is implicit in the stored
    -- `tables` slice.
    last_analysis_snapshot  JSONB,

    -- Per-table SchemaFingerprint map keyed by table name. Stored
    -- as `{ "<table>": { "hash": "<hex>", "computed_at": "<iso>" } }`.
    -- A re-scan compares the live fingerprint against this map and
    -- only re-introspects tables whose hash differs — turning a
    -- "refresh" gesture into a delta scan.
    schema_fingerprints     JSONB NOT NULL DEFAULT '{}'::jsonb,

    -- Wall-clock timestamp of the most-recent successful analyze_*
    -- call. NULL when nothing has been analysed yet. Used by the
    -- UI to render "last analysed N hours ago" without parsing the
    -- snapshot blob.
    last_analyzed_at        TIMESTAMPTZ,

    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),

    -- Within a workspace, `source_id` must be unique — ontology
    -- mappings reference it by that string and the planner would
    -- not know which adapter to resolve on collision.
    CONSTRAINT data_sources_ws_source_unique UNIQUE (workspace_id, source_id)
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

-- ============================================================================
-- Ontology store — Level 1 (identity + version snapshot)
-- ============================================================================

CREATE TABLE ontologies (
    id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    -- Stable identity across versions. External systems (quality
    -- rules, saved queries, design projects) reference an ontology
    -- via lineage_id; two ontologies with the same `name` in
    -- different workspaces are still distinct because of RLS +
    -- `workspace_id`.
    -- TEXT rather than UUID because the ontology's `lineage_id`
    -- mirrors `OntologyIR.lineage_id` which is a `String`;
    -- LLM-generated lineage tags and imported ontologies may
    -- carry non-UUID identifiers.
    lineage_id     TEXT NOT NULL,
    workspace_id   UUID NOT NULL DEFAULT (current_setting('app.workspace_id', true))::uuid
                        REFERENCES workspaces(id) ON DELETE CASCADE,
    -- Canonical short identifier (`"E-commerce"`, `"Healthcare"`).
    -- Used as the URI fragment in OWL / SHACL exports, the lineage
    -- handle external systems reference, and the workspace-scoped
    -- uniqueness key. Not required to be Cypher-safe — the ontology
    -- itself is not a graph label. Single language by design;
    -- locale-aware label lives in `display_name`.
    name           TEXT NOT NULL,
    -- Locale-aware human label. `LocalizedText` JSONB for the same
    -- round-trip reasons as `description`. Empty default when the
    -- caller doesn't supply one — consumers fall back to `name`.
    display_name   JSONB NOT NULL DEFAULT '{"default":"","translations":{}}'::jsonb,
    -- LocalizedText — stored as JSONB so the admin UI can round-trip
    -- `{default: "...", translations: {...}}` without a second table.
    description    JSONB NOT NULL DEFAULT '{"default":"","translations":{}}'::jsonb,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now(),

    -- lineage_id is globally unique — it's the key external
    -- systems reference and must not collide even across tenants
    -- (a quality rule might be tenant-scoped but the lineage_id
    -- it names is looked up globally before RLS filters the row
    -- set visible to that tenant).
    CONSTRAINT ontologies_lineage_id_uq UNIQUE (lineage_id),
    -- Within a workspace a name is unique. Two workspaces can each
    -- have an "E-commerce" ontology.
    CONSTRAINT ontologies_ws_name_uq UNIQUE (workspace_id, name),
    -- Composite UNIQUE needed as the FK target for
    -- `ontology_drafts(workspace_id, ontology_id)` and
    -- `query_executions(workspace_id, ontology_id)` — PostgreSQL
    -- requires a matching unique index for multi-column FKs, and
    -- the primary key on `id` alone does not satisfy it.
    CONSTRAINT ontologies_ws_id_uq UNIQUE (workspace_id, id)
);

CREATE INDEX ontologies_workspace_idx ON ontologies (workspace_id, created_at DESC);


-- --- Level 1 : ontology_version_snapshots ----------------------------------
--
-- A pointer-set version model. Each row declares "version V of
-- ontology O, comprising the entity set named in
-- `ontology_version_entities` (populated in Λ-2)". The content
-- itself lives in the immutable Level 2 entity store.
--
-- Bitemporal columns follow the standard four-field pattern:
--
--   valid_from / valid_to  — business time. "When was this version
--                            considered the *live* version of the
--                            ontology?" Open-ended when `valid_to`
--                            is NULL.
--   sys_from   / sys_to    — system time. "When did this row
--                            actually exist in our storage?"
--                            A version may be retroactively
--                            superseded; `sys_to` records that.
--
-- `parent_version_id` is nullable and points at the predecessor
-- in the lineage's version chain. A linear history leaves a
-- simple chain; a future branching workflow (multi-author
-- proposals merged later) extends to a DAG without schema change.
--
-- `committed_by` captures who applied the edit; `commit_message`
-- is the human-authored summary — both render in the admin UI's
-- version history panel.

CREATE TABLE ontology_version_snapshots (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    ontology_id         UUID NOT NULL
                             REFERENCES ontologies(id) ON DELETE CASCADE,
    -- Semver-ish free-form string. Integer-only deployments use
    -- stringified counters ("1", "2", ...); semver-adopting teams
    -- use "1.2.3". The column is TEXT so both shapes round-trip.
    version             TEXT NOT NULL,

    -- Bitemporal — see header comment.
    valid_from          TIMESTAMPTZ NOT NULL DEFAULT now(),
    valid_to            TIMESTAMPTZ,
    sys_from            TIMESTAMPTZ NOT NULL DEFAULT now(),
    sys_to              TIMESTAMPTZ,

    -- Version-chain parent — NULL for the first version of a
    -- lineage. A branching workflow (future) adds multiple
    -- children pointing at the same parent.
    parent_version_id   UUID REFERENCES ontology_version_snapshots(id)
                              ON DELETE SET NULL,

    committed_by        TEXT NOT NULL,
    commit_message      TEXT NOT NULL DEFAULT '',
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),

    workspace_id        UUID NOT NULL DEFAULT (current_setting('app.workspace_id', true))::uuid
                             REFERENCES workspaces(id) ON DELETE CASCADE,

    -- Each (ontology_id, version) is unique. Two versions cannot
    -- carry the same version tag — the commit path enforces
    -- monotonic version assignment.
    CONSTRAINT ontology_version_snapshots_ont_ver_uq
        UNIQUE (ontology_id, version)
);

CREATE INDEX ontology_version_snapshots_ontology_idx
    ON ontology_version_snapshots (ontology_id, created_at DESC);

-- "current version" lookup fast-path: latest row per ontology
-- where valid_to IS NULL. Covered partial index so the common
-- case (load latest) reads a single btree page.
CREATE INDEX ontology_version_snapshots_current_idx
    ON ontology_version_snapshots (ontology_id)
    WHERE valid_to IS NULL;


-- --- RLS -------------------------------------------------------------------
--
-- Identical four-statement boilerplate used by every other
-- workspace-scoped table. See 0004_rls.sql for the pattern.

ALTER TABLE ontologies ENABLE ROW LEVEL SECURITY;
ALTER TABLE ontologies FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON ontologies
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON ontologies
    USING (current_setting('app.system_bypass', true) = 'true');

ALTER TABLE ontology_version_snapshots ENABLE ROW LEVEL SECURITY;
ALTER TABLE ontology_version_snapshots FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON ontology_version_snapshots
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON ontology_version_snapshots
    USING (current_setting('app.system_bypass', true) = 'true');

-- Cross-table foreign keys that need `ontologies` to exist — the
-- table-creation order means these must land after this section.
ALTER TABLE ontology_drafts
    ADD CONSTRAINT ontology_drafts_ontology_ws_fk
        FOREIGN KEY (workspace_id, ontology_id)
        REFERENCES ontologies(workspace_id, id)
        ON DELETE SET NULL;

ALTER TABLE query_executions
    ADD CONSTRAINT query_executions_ontology_ws_fk
        FOREIGN KEY (workspace_id, ontology_id)
        REFERENCES ontologies(workspace_id, id)
        ON DELETE RESTRICT;


-- ============================================================================
-- Ontology store — Level 2 (content-addressed entity versions)
-- ============================================================================

CREATE TYPE ontology_entity_kind AS ENUM (
    'ontology_header',
    -- Topology
    'node_type',
    'edge_type',
    'index_def',
    'interface',
    -- A property defined on a node_type or edge_type. Properties carry
    -- their own stable id and are referenced standalone from the
    -- materialised neighbour graph (`ontology_entity_neighbors`) and
    -- from search-index rows — registering them here keeps every
    -- `entity_kind` cast safe regardless of which surface emits it.
    -- The content-addressed store does NOT extract properties as
    -- separate `ExtractedEntity`s (they live inside the parent type's
    -- payload); this enum exists for the materialised denormalised
    -- views that DO need to anchor edges at property granularity.
    'property',
    -- Mapping
    'object_mapping',
    'link_mapping',
    'property_mapping',
    -- Governance
    'rule',
    'data_quality',
    'action',
    'provenance',
    -- Behaviour
    'function',
    'metric',
    'enrichment',
    -- Vocabulary + value semantics
    'glossary_term',
    'taxonomy',
    'code_system',
    -- Individual `CodedValue` rows nested inside a `code_system`.
    -- Carries its own stable id; the materialised hierarchy table
    -- (`ontology_entity_hierarchy`) anchors the SKOS-style
    -- `code_system_broader` walk at this granularity so consumers
    -- can ask "which codes are below CODE_X?" without joining
    -- through the parent system. Like `property` above, not emitted
    -- as a top-level `ExtractedEntity` — lives inside the parent
    -- `CodeSystemDef`'s payload in the content-addressed store.
    'coded_value',
    'value_set',
    'notation_pattern',
    'concept_map',
    'value_range_set',
    -- Φ3 — per-column distribution snapshot
    'column_profile'
);


-- --- Immutable content-addressed store -------------------------------------
--
-- Primary key is the entity's content hash. A new entity with
-- the same canonical JSON as an existing one collapses into a
-- single row; the `version_entities` pointer table is where
-- versions diverge.

CREATE TABLE ontology_entity_versions (
    -- SHA-256 hex of the entity's canonical JSON. 64 hex chars.
    entity_hash   TEXT PRIMARY KEY
                       CHECK (entity_hash ~ '^[0-9a-f]{64}$'),
    entity_kind   ontology_entity_kind NOT NULL,
    -- Canonical JSON of the entity. MUST be produced by the
    -- RFC-8785 JSON Canonicalization Scheme (sorted keys, no
    -- spurious whitespace) — otherwise two logically equivalent
    -- edits would hash differently and cross-version dedup fails.
    -- The Rust entity extractor (Λ-3) owns this invariant.
    content       JSONB NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),

    -- Intentionally NOT workspace-scoped. Content-addressed
    -- entities are tenant-neutral: identical bytes across
    -- workspaces share one row. Workspace scoping lives on the
    -- pointer table below, which joins to `ontologies` (which is
    -- workspace-scoped).
    --
    -- This mirrors how Git objects work: a blob `abc123` is the
    -- same blob regardless of which repo references it, and the
    -- repo-scoped refs (branches, tags) point at the global
    -- object store. Same pattern here with versions → entities.
    CONSTRAINT ontology_entity_versions_kind_chk CHECK (true)
);

-- Query by kind is a very common pattern (list all code_systems,
-- find all node_types missing mappings, etc.). Partial index
-- per kind avoids scanning the whole content-addressed store.
CREATE INDEX ontology_entity_versions_kind_idx
    ON ontology_entity_versions (entity_kind);

-- JSONB GIN on content for admin / debug lookup. Not on the hot
-- path (Level 3 materialised views are). Kept here so a direct
-- "find the entity that has this property" query from psql works.
CREATE INDEX ontology_entity_versions_content_gin
    ON ontology_entity_versions USING gin (content);


-- --- Version-to-entity pointer set -----------------------------------------
--
-- M:N between versions and entities. A version's "content" is
-- the set of rows here with `version_id = V`. An entity appears
-- in the set exactly once per `(kind, logical_id)`, via its
-- current hash.
--
-- `entity_logical_id` is the stable Rust newtype id (UUID-shaped).
-- Tracking rename / evolution: fetch all snapshots that contain
-- `(kind=node_type, logical_id=L)` ordered by version — each
-- distinct hash is a rewrite of that node type.
--
-- The DELETE cascade on `version_id` lets the admin-level
-- "remove a version" operation clean its pointers without
-- touching the immutable entity store. The entity rows stay
-- because they may still be referenced by sibling versions; a
-- future garbage collector (not yet in scope) can sweep
-- orphaned content-addressed rows.

CREATE TABLE ontology_version_entities (
    version_id        UUID NOT NULL
                            REFERENCES ontology_version_snapshots(id)
                            ON DELETE CASCADE,
    entity_kind       ontology_entity_kind NOT NULL,
    -- Stable author-assigned id per entity kind. TEXT because
    -- every `XxxId` newtype in ox-ontology wraps a `String`
    -- (LLM-generated ids, externally-imported codes, etc. aren't
    -- always UUIDs). The header kind uses the owning
    -- `OntologyIR.id` as its logical_id — singleton per version.
    entity_logical_id TEXT NOT NULL,
    entity_hash       TEXT NOT NULL
                           REFERENCES ontology_entity_versions(entity_hash),

    workspace_id      UUID NOT NULL DEFAULT (current_setting('app.workspace_id', true))::uuid
                           REFERENCES workspaces(id) ON DELETE CASCADE,

    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),

    PRIMARY KEY (version_id, entity_kind, entity_logical_id)
);

-- Fast "load all entities for version V" — covers the hot-path
-- hydration read.
CREATE INDEX ontology_version_entities_version_idx
    ON ontology_version_entities (version_id);

-- "History of logical_id L across versions" — used by rename
-- tracker (TemporalRewriter) and by the admin UI's version-diff
-- view. Indexed by (kind, logical_id) so the history read is
-- cheap regardless of how many versions exist.
CREATE INDEX ontology_version_entities_logical_idx
    ON ontology_version_entities (entity_kind, entity_logical_id);


-- --- RLS -------------------------------------------------------------------
--
-- `ontology_entity_versions` is tenant-neutral by design (see
-- the comment in the table definition) — no RLS. The pointer
-- table IS workspace-scoped: a tenant can only enumerate
-- pointers in their own workspace, so they can only hydrate the
-- entities they have access to (the global dedup does not leak
-- cross-tenant content because there's no way to enumerate an
-- entity without first knowing a version that references it).

ALTER TABLE ontology_version_entities ENABLE ROW LEVEL SECURITY;
ALTER TABLE ontology_version_entities FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON ontology_version_entities
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON ontology_version_entities
    USING (current_setting('app.system_bypass', true) = 'true');

-- ============================================================================
-- Ontology store — Level 3 flat indexes
-- ============================================================================

CREATE TABLE ontology_node_type_index (
    version_id      UUID NOT NULL
                         REFERENCES ontology_version_snapshots(id)
                         ON DELETE CASCADE,
    logical_id      TEXT NOT NULL,
    entity_hash     TEXT NOT NULL
                         REFERENCES ontology_entity_versions(entity_hash),
    label           TEXT NOT NULL,
    deprecated_at   TIMESTAMPTZ,
    workspace_id    UUID NOT NULL DEFAULT (current_setting('app.workspace_id', true))::uuid
                         REFERENCES workspaces(id) ON DELETE CASCADE,
    PRIMARY KEY (version_id, logical_id)
);
CREATE INDEX ontology_node_type_index_label_idx
    ON ontology_node_type_index (version_id, label);
CREATE INDEX ontology_node_type_index_active_idx
    ON ontology_node_type_index (version_id)
    WHERE deprecated_at IS NULL;


-- --- edge_type -------------------------------------------------------------

CREATE TABLE ontology_edge_type_index (
    version_id        UUID NOT NULL
                           REFERENCES ontology_version_snapshots(id)
                           ON DELETE CASCADE,
    logical_id        TEXT NOT NULL,
    entity_hash       TEXT NOT NULL
                           REFERENCES ontology_entity_versions(entity_hash),
    label             TEXT NOT NULL,
    source_type_id    TEXT NOT NULL,
    target_type_id    TEXT NOT NULL,
    deprecated_at     TIMESTAMPTZ,
    workspace_id      UUID NOT NULL DEFAULT (current_setting('app.workspace_id', true))::uuid
                           REFERENCES workspaces(id) ON DELETE CASCADE,
    PRIMARY KEY (version_id, logical_id)
);
CREATE INDEX ontology_edge_type_index_label_idx
    ON ontology_edge_type_index (version_id, label);
CREATE INDEX ontology_edge_type_index_source_idx
    ON ontology_edge_type_index (version_id, source_type_id);
CREATE INDEX ontology_edge_type_index_target_idx
    ON ontology_edge_type_index (version_id, target_type_id);


-- --- property --------------------------------------------------------------
--
-- Properties are NESTED in node_type / edge_type at the IR level
-- (they're not top-level entities in Level 2), but they carry the
-- richest facet set for NL2SQL prompt enrichment — aggregation_role,
-- semantic_type, pii_kind — so the materialisation here lets the
-- prompt-builder ask "show me all Measure-role properties" in one
-- index seek.
--
-- Semantic bindings (value-set / notation-pattern / value-range /
-- glossary / code-system) are normalised into the sibling
-- `ontology_property_binding` table — multi-binding properties are
-- first-class, strength + concept-map + temporal scope all preserved.

CREATE TABLE ontology_property_index (
    version_id             UUID NOT NULL
                                REFERENCES ontology_version_snapshots(id)
                                ON DELETE CASCADE,
    owner_kind             ontology_entity_kind NOT NULL,      -- node_type | edge_type
    owner_logical_id       TEXT NOT NULL,
    logical_id             TEXT NOT NULL,                      -- property id
    entity_hash            TEXT NOT NULL,                       -- owner's hash
    key                    TEXT NOT NULL,
    property_type          TEXT NOT NULL,                       -- "string", "int", etc.
    nullable               BOOLEAN NOT NULL,
    is_localized           BOOLEAN NOT NULL,
    aggregation_role       TEXT,                                -- measure | dimension | attribute | identifier
    semantic_type          TEXT,
    pii_kind               TEXT,
    unit_id                TEXT,
    deprecated_at          TIMESTAMPTZ,
    workspace_id           UUID NOT NULL DEFAULT (current_setting('app.workspace_id', true))::uuid
                                REFERENCES workspaces(id) ON DELETE CASCADE,
    PRIMARY KEY (version_id, owner_kind, owner_logical_id, logical_id)
);
CREATE INDEX ontology_property_index_version_idx
    ON ontology_property_index (version_id);

-- One row per binding on a property; targets a registry entry by
-- `(target_kind, target_id)`. Multi-binding properties produce
-- multiple rows. Strength + concept-map + temporal-window fields
-- carry every dimension the in-memory `PropertyBinding` does, so a
-- registry-keyed search ("which properties bind to value-set X with
-- Required strength") lands in one indexed seek.
CREATE TABLE ontology_property_binding (
    version_id             UUID NOT NULL
                                REFERENCES ontology_version_snapshots(id)
                                ON DELETE CASCADE,
    owner_kind             ontology_entity_kind NOT NULL,
    owner_logical_id       TEXT NOT NULL,
    property_logical_id    TEXT NOT NULL,
    -- ordinal index inside the property's `bindings` Vec — preserves
    -- author intent when two bindings would both classify a value
    -- (consumers honour the first match).
    ordinal                INTEGER NOT NULL,
    -- snake_case discriminator: value_set | code_system |
    -- notation_pattern | value_range | glossary
    target_kind            TEXT NOT NULL,
    target_id              TEXT NOT NULL,
    -- snake_case strength: required | preferred | extensible | example
    strength               TEXT NOT NULL,
    concept_map_id         TEXT,
    valid_from             TIMESTAMPTZ,
    valid_to               TIMESTAMPTZ,
    workspace_id           UUID NOT NULL DEFAULT (current_setting('app.workspace_id', true))::uuid
                                REFERENCES workspaces(id) ON DELETE CASCADE,
    PRIMARY KEY (version_id, owner_kind, owner_logical_id, property_logical_id, ordinal)
);
-- Lookup by registry target — answers the admin UI's
-- "which properties use this value set?" question per snapshot.
-- Workspace prefix keeps multi-tenant scans tight; RLS additionally
-- enforces row-level isolation, but the index prefix lets the
-- planner narrow before the policy check.
CREATE INDEX ontology_property_binding_target_idx
    ON ontology_property_binding (workspace_id, version_id, target_kind, target_id);
-- Lookup by version + strength — Required-binding sweep for the
-- write-time validator's pre-flight cache.
CREATE INDEX ontology_property_binding_strength_idx
    ON ontology_property_binding (workspace_id, version_id, strength);


-- --- interface --------------------------------------------------------------

CREATE TABLE ontology_interface_index (
    version_id    UUID NOT NULL
                       REFERENCES ontology_version_snapshots(id)
                       ON DELETE CASCADE,
    logical_id    TEXT NOT NULL,
    entity_hash   TEXT NOT NULL
                       REFERENCES ontology_entity_versions(entity_hash),
    label         TEXT NOT NULL,
    workspace_id  UUID NOT NULL DEFAULT (current_setting('app.workspace_id', true))::uuid
                       REFERENCES workspaces(id) ON DELETE CASCADE,
    PRIMARY KEY (version_id, logical_id)
);
CREATE INDEX ontology_interface_index_label_idx
    ON ontology_interface_index (version_id, label);


-- --- object_mapping --------------------------------------------------------

CREATE TABLE ontology_object_mapping_index (
    version_id     UUID NOT NULL
                        REFERENCES ontology_version_snapshots(id)
                        ON DELETE CASCADE,
    logical_id     TEXT NOT NULL,
    entity_hash    TEXT NOT NULL
                        REFERENCES ontology_entity_versions(entity_hash),
    node_type_id   TEXT NOT NULL,
    source_id      TEXT NOT NULL,
    precedence     SMALLINT NOT NULL DEFAULT 0,
    workspace_id   UUID NOT NULL DEFAULT (current_setting('app.workspace_id', true))::uuid
                        REFERENCES workspaces(id) ON DELETE CASCADE,
    PRIMARY KEY (version_id, logical_id)
);
CREATE INDEX ontology_object_mapping_index_node_idx
    ON ontology_object_mapping_index (version_id, node_type_id, precedence DESC);
CREATE INDEX ontology_object_mapping_index_source_idx
    ON ontology_object_mapping_index (version_id, source_id);


-- --- link_mapping ----------------------------------------------------------

CREATE TABLE ontology_link_mapping_index (
    version_id     UUID NOT NULL
                        REFERENCES ontology_version_snapshots(id)
                        ON DELETE CASCADE,
    logical_id     TEXT NOT NULL,
    entity_hash    TEXT NOT NULL
                        REFERENCES ontology_entity_versions(entity_hash),
    edge_type_id   TEXT NOT NULL,
    kind           TEXT NOT NULL,              -- foreign_key | bridge | computed | federated
    cardinality    TEXT NOT NULL,              -- one_to_one | one_to_many | many_to_one | many_to_many
    precedence     SMALLINT NOT NULL DEFAULT 0,
    workspace_id   UUID NOT NULL DEFAULT (current_setting('app.workspace_id', true))::uuid
                        REFERENCES workspaces(id) ON DELETE CASCADE,
    PRIMARY KEY (version_id, logical_id)
);
CREATE INDEX ontology_link_mapping_index_edge_idx
    ON ontology_link_mapping_index (version_id, edge_type_id, precedence DESC);


-- --- code_system -----------------------------------------------------------

CREATE TABLE ontology_code_system_index (
    version_id    UUID NOT NULL
                       REFERENCES ontology_version_snapshots(id)
                       ON DELETE CASCADE,
    logical_id    TEXT NOT NULL,
    entity_hash   TEXT NOT NULL
                       REFERENCES ontology_entity_versions(entity_hash),
    name          TEXT NOT NULL,
    uri           TEXT,
    kind          TEXT NOT NULL,                -- internal | external
    hierarchical  BOOLEAN NOT NULL DEFAULT FALSE,
    workspace_id  UUID NOT NULL DEFAULT (current_setting('app.workspace_id', true))::uuid
                       REFERENCES workspaces(id) ON DELETE CASCADE,
    PRIMARY KEY (version_id, logical_id)
);
CREATE INDEX ontology_code_system_index_name_idx
    ON ontology_code_system_index (version_id, name);


-- --- coded_value -----------------------------------------------------------
--
-- Nested in code_system at the IR level, lifted to a flat table
-- because value-lookup is the single highest-volume query in the
-- value-semantics layer. `broader_id` stays so the hierarchy-walk
-- helper resolves "all codes below `Region.KR`" in a recursive
-- CTE over this single table.

CREATE TABLE ontology_coded_value_index (
    version_id        UUID NOT NULL
                           REFERENCES ontology_version_snapshots(id)
                           ON DELETE CASCADE,
    logical_id        TEXT NOT NULL,
    entity_hash       TEXT NOT NULL,             -- code system's hash
    code_system_id    TEXT NOT NULL,
    code              TEXT NOT NULL,
    broader_id        TEXT,
    deprecated_at     TIMESTAMPTZ,
    workspace_id      UUID NOT NULL DEFAULT (current_setting('app.workspace_id', true))::uuid
                           REFERENCES workspaces(id) ON DELETE CASCADE,
    PRIMARY KEY (version_id, code_system_id, logical_id)
);
CREATE INDEX ontology_coded_value_index_code_idx
    ON ontology_coded_value_index (version_id, code_system_id, code);
CREATE INDEX ontology_coded_value_index_broader_idx
    ON ontology_coded_value_index (version_id, broader_id)
    WHERE broader_id IS NOT NULL;


-- --- value_set -------------------------------------------------------------

CREATE TABLE ontology_value_set_index (
    version_id   UUID NOT NULL
                      REFERENCES ontology_version_snapshots(id)
                      ON DELETE CASCADE,
    logical_id   TEXT NOT NULL,
    entity_hash  TEXT NOT NULL
                      REFERENCES ontology_entity_versions(entity_hash),
    name         TEXT NOT NULL,
    workspace_id UUID NOT NULL DEFAULT (current_setting('app.workspace_id', true))::uuid
                      REFERENCES workspaces(id) ON DELETE CASCADE,
    PRIMARY KEY (version_id, logical_id)
);
CREATE INDEX ontology_value_set_index_name_idx
    ON ontology_value_set_index (version_id, name);


-- --- notation_pattern ------------------------------------------------------

CREATE TABLE ontology_notation_pattern_index (
    version_id   UUID NOT NULL
                      REFERENCES ontology_version_snapshots(id)
                      ON DELETE CASCADE,
    logical_id   TEXT NOT NULL,
    entity_hash  TEXT NOT NULL
                      REFERENCES ontology_entity_versions(entity_hash),
    name         TEXT NOT NULL,
    workspace_id UUID NOT NULL DEFAULT (current_setting('app.workspace_id', true))::uuid
                      REFERENCES workspaces(id) ON DELETE CASCADE,
    PRIMARY KEY (version_id, logical_id)
);
CREATE INDEX ontology_notation_pattern_index_name_idx
    ON ontology_notation_pattern_index (version_id, name);


-- --- concept_map -----------------------------------------------------------

CREATE TABLE ontology_concept_map_index (
    version_id          UUID NOT NULL
                             REFERENCES ontology_version_snapshots(id)
                             ON DELETE CASCADE,
    logical_id          TEXT NOT NULL,
    entity_hash         TEXT NOT NULL
                             REFERENCES ontology_entity_versions(entity_hash),
    name                TEXT NOT NULL,
    source_system_id    TEXT NOT NULL,
    target_system_id    TEXT NOT NULL,
    workspace_id        UUID NOT NULL DEFAULT (current_setting('app.workspace_id', true))::uuid
                             REFERENCES workspaces(id) ON DELETE CASCADE,
    PRIMARY KEY (version_id, logical_id)
);
CREATE INDEX ontology_concept_map_index_source_idx
    ON ontology_concept_map_index (version_id, source_system_id);
CREATE INDEX ontology_concept_map_index_target_idx
    ON ontology_concept_map_index (version_id, target_system_id);


-- --- value_range_set -------------------------------------------------------

CREATE TABLE ontology_value_range_set_index (
    version_id   UUID NOT NULL
                      REFERENCES ontology_version_snapshots(id)
                      ON DELETE CASCADE,
    logical_id   TEXT NOT NULL,
    entity_hash  TEXT NOT NULL
                      REFERENCES ontology_entity_versions(entity_hash),
    name         TEXT NOT NULL,
    workspace_id UUID NOT NULL DEFAULT (current_setting('app.workspace_id', true))::uuid
                      REFERENCES workspaces(id) ON DELETE CASCADE,
    PRIMARY KEY (version_id, logical_id)
);
CREATE INDEX ontology_value_range_set_index_name_idx
    ON ontology_value_range_set_index (version_id, name);


-- --- glossary_term ---------------------------------------------------------

CREATE TABLE ontology_glossary_term_index (
    version_id        UUID NOT NULL
                           REFERENCES ontology_version_snapshots(id)
                           ON DELETE CASCADE,
    logical_id        TEXT NOT NULL,
    entity_hash       TEXT NOT NULL
                           REFERENCES ontology_entity_versions(entity_hash),
    term              TEXT NOT NULL,
    category          TEXT,
    related_terms     JSONB NOT NULL DEFAULT '[]'::jsonb,
    workspace_id      UUID NOT NULL DEFAULT (current_setting('app.workspace_id', true))::uuid
                           REFERENCES workspaces(id) ON DELETE CASCADE,
    PRIMARY KEY (version_id, logical_id)
);
CREATE INDEX ontology_glossary_term_index_term_idx
    ON ontology_glossary_term_index (version_id, term);
CREATE INDEX ontology_glossary_term_index_related_idx
    ON ontology_glossary_term_index USING GIN (related_terms);


-- --- rule ------------------------------------------------------------------

CREATE TABLE ontology_rule_index (
    version_id    UUID NOT NULL
                       REFERENCES ontology_version_snapshots(id)
                       ON DELETE CASCADE,
    logical_id    TEXT NOT NULL,
    entity_hash   TEXT NOT NULL
                       REFERENCES ontology_entity_versions(entity_hash),
    kind          TEXT NOT NULL,
    severity      TEXT NOT NULL,
    workspace_id  UUID NOT NULL DEFAULT (current_setting('app.workspace_id', true))::uuid
                       REFERENCES workspaces(id) ON DELETE CASCADE,
    PRIMARY KEY (version_id, logical_id)
);
CREATE INDEX ontology_rule_index_kind_idx
    ON ontology_rule_index (version_id, kind);


-- --- function --------------------------------------------------------------

CREATE TABLE ontology_function_index (
    version_id   UUID NOT NULL
                      REFERENCES ontology_version_snapshots(id)
                      ON DELETE CASCADE,
    logical_id   TEXT NOT NULL,
    entity_hash  TEXT NOT NULL
                      REFERENCES ontology_entity_versions(entity_hash),
    name         TEXT NOT NULL,
    purity       TEXT NOT NULL,
    workspace_id UUID NOT NULL DEFAULT (current_setting('app.workspace_id', true))::uuid
                      REFERENCES workspaces(id) ON DELETE CASCADE,
    PRIMARY KEY (version_id, logical_id)
);
CREATE INDEX ontology_function_index_name_idx
    ON ontology_function_index (version_id, name);


-- --- metric ----------------------------------------------------------------

CREATE TABLE ontology_metric_index (
    version_id       UUID NOT NULL
                          REFERENCES ontology_version_snapshots(id)
                          ON DELETE CASCADE,
    logical_id       TEXT NOT NULL,
    entity_hash      TEXT NOT NULL
                          REFERENCES ontology_entity_versions(entity_hash),
    name             TEXT NOT NULL,
    temporal_grain   TEXT,
    workspace_id     UUID NOT NULL DEFAULT (current_setting('app.workspace_id', true))::uuid
                          REFERENCES workspaces(id) ON DELETE CASCADE,
    PRIMARY KEY (version_id, logical_id)
);
CREATE INDEX ontology_metric_index_name_idx
    ON ontology_metric_index (version_id, name);


-- --- RLS (uniform four-statement pattern) ---------------------------------

DO $$
DECLARE tbl TEXT;
BEGIN
    FOR tbl IN
        SELECT unnest(ARRAY[
            'ontology_node_type_index',
            'ontology_edge_type_index',
            'ontology_property_index',
            'ontology_property_binding',
            'ontology_interface_index',
            'ontology_object_mapping_index',
            'ontology_link_mapping_index',
            'ontology_code_system_index',
            'ontology_coded_value_index',
            'ontology_value_set_index',
            'ontology_notation_pattern_index',
            'ontology_concept_map_index',
            'ontology_value_range_set_index',
            'ontology_glossary_term_index',
            'ontology_rule_index',
            'ontology_function_index',
            'ontology_metric_index'
        ])
    LOOP
        EXECUTE format('ALTER TABLE %I ENABLE ROW LEVEL SECURITY', tbl);
        EXECUTE format('ALTER TABLE %I FORCE ROW LEVEL SECURITY', tbl);
        EXECUTE format(
            'CREATE POLICY ws_isolation ON %I
                USING (workspace_id = current_setting(''app.workspace_id'', true)::uuid)
                WITH CHECK (workspace_id = current_setting(''app.workspace_id'', true)::uuid)',
            tbl
        );
        EXECUTE format(
            'CREATE POLICY system_bypass ON %I
                USING (current_setting(''app.system_bypass'', true) = ''true'')',
            tbl
        );
    END LOOP;
END$$;

-- ============================================================================
-- Ontology navigation (neighbors + hierarchy)
-- ============================================================================

CREATE TABLE ontology_entity_neighbors (
    version_id        UUID NOT NULL
                           REFERENCES ontology_version_snapshots(id)
                           ON DELETE CASCADE,
    from_kind         ontology_entity_kind NOT NULL,
    from_logical_id   TEXT NOT NULL,
    to_kind           ontology_entity_kind NOT NULL,
    to_logical_id     TEXT NOT NULL,
    relation_kind     TEXT NOT NULL,
    workspace_id      UUID NOT NULL DEFAULT (current_setting('app.workspace_id', true))::uuid
                           REFERENCES workspaces(id) ON DELETE CASCADE,

    PRIMARY KEY (version_id, from_kind, from_logical_id, to_kind, to_logical_id, relation_kind)
);

-- Forward traversal: "from this entity, what does it reference?"
-- Covered by the primary key.

-- Reverse traversal: "what references this entity?" — the admin
-- UI uses this for the inverse sidebar ("3 Properties reference
-- this ValueSet").
CREATE INDEX ontology_entity_neighbors_reverse_idx
    ON ontology_entity_neighbors
       (version_id, to_kind, to_logical_id, relation_kind);


-- --- Hierarchical closure table --------------------------------------------
--
-- One row per (ancestor, descendant) pair where descendant is
-- reachable from ancestor by walking the hierarchical relation
-- indicated by `relation_kind`. Relation kinds covered:
--
--   code_system_broader       CodedValue.broader_id chain inside a system
--   glossary_term_broader     GlossaryTermDef.related_terms[Broader] chain
--   interface_implements      NodeType → Interface (implements)
--
-- `depth = 0` is the self-reference (every entity is its own
-- ancestor with depth 0) — makes "ancestors inclusive" queries a
-- single table scan instead of a UNION.

CREATE TABLE ontology_entity_hierarchy (
    version_id         UUID NOT NULL
                            REFERENCES ontology_version_snapshots(id)
                            ON DELETE CASCADE,
    relation_kind      TEXT NOT NULL,
    ancestor_kind      ontology_entity_kind NOT NULL,
    ancestor_logical_id TEXT NOT NULL,
    descendant_kind    ontology_entity_kind NOT NULL,
    descendant_logical_id TEXT NOT NULL,
    -- Depth in walk-steps from ancestor to descendant. 0 = self.
    depth              INTEGER NOT NULL CHECK (depth >= 0),
    workspace_id       UUID NOT NULL DEFAULT (current_setting('app.workspace_id', true))::uuid
                            REFERENCES workspaces(id) ON DELETE CASCADE,

    PRIMARY KEY (version_id, relation_kind,
                 ancestor_kind, ancestor_logical_id,
                 descendant_kind, descendant_logical_id)
);

-- Descendants-of lookup: "given this code, all codes below it in
-- the hierarchy".
CREATE INDEX ontology_entity_hierarchy_descendants_idx
    ON ontology_entity_hierarchy
       (version_id, relation_kind, ancestor_kind, ancestor_logical_id, depth);

-- Ancestors-of lookup: "given this code, all ancestors (up to
-- the root)". Used by the temporal rewriter when it needs to
-- resolve a renamed property through its lineage chain.
CREATE INDEX ontology_entity_hierarchy_ancestors_idx
    ON ontology_entity_hierarchy
       (version_id, relation_kind, descendant_kind, descendant_logical_id, depth);


-- --- RLS --------------------------------------------------------------------

ALTER TABLE ontology_entity_neighbors ENABLE ROW LEVEL SECURITY;
ALTER TABLE ontology_entity_neighbors FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON ontology_entity_neighbors
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON ontology_entity_neighbors
    USING (current_setting('app.system_bypass', true) = 'true');

ALTER TABLE ontology_entity_hierarchy ENABLE ROW LEVEL SECURITY;
ALTER TABLE ontology_entity_hierarchy FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON ontology_entity_hierarchy
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON ontology_entity_hierarchy
    USING (current_setting('app.system_bypass', true) = 'true');

-- ============================================================================
-- Ontology search + embedding vectors
-- ============================================================================

CREATE TABLE ontology_entity_search_vector (
    version_id    UUID NOT NULL
                       REFERENCES ontology_version_snapshots(id)
                       ON DELETE CASCADE,
    entity_kind   ontology_entity_kind NOT NULL,
    logical_id    TEXT NOT NULL,
    doc           TEXT NOT NULL,
    tsv           tsvector NOT NULL,
    workspace_id  UUID NOT NULL DEFAULT (current_setting('app.workspace_id', true))::uuid
                       REFERENCES workspaces(id) ON DELETE CASCADE,

    PRIMARY KEY (version_id, entity_kind, logical_id)
);

CREATE INDEX ontology_entity_search_vector_tsv_idx
    ON ontology_entity_search_vector USING gin (tsv);
CREATE INDEX ontology_entity_search_vector_trgm_idx
    ON ontology_entity_search_vector USING gin (doc gin_trgm_ops);


-- --- Semantic embedding ---------------------------------------------------
--
-- `embedding` is populated async by a batched background job
-- (the Gemini Embedding 2 API round-trip would be latency
-- prohibitive on the commit hot path). Rows exist here only
-- after a successful embedding fetch; callers that hit a cold
-- row fall back to full-text / trigram.
--
-- Dimension 1536 matches Gemini Embedding 2 Preview's MRL-
-- reduced output; HNSW index over `vector_cosine_ops` serves
-- top-K cosine-similarity queries in sub-20ms against the
-- Ontoserver-scale target (100K entities × 1536 dims).

CREATE TABLE ontology_entity_embedding (
    version_id    UUID NOT NULL
                       REFERENCES ontology_version_snapshots(id)
                       ON DELETE CASCADE,
    entity_kind   ontology_entity_kind NOT NULL,
    logical_id    TEXT NOT NULL,
    embedding     vector(1536),
    workspace_id  UUID NOT NULL DEFAULT (current_setting('app.workspace_id', true))::uuid
                       REFERENCES workspaces(id) ON DELETE CASCADE,

    PRIMARY KEY (version_id, entity_kind, logical_id)
);

-- HNSW index — accelerates `<=>` cosine-distance top-K lookups.
-- Partial index skips rows where `embedding IS NULL` (cold rows
-- haven't been populated yet), keeping the index small.
CREATE INDEX ontology_entity_embedding_hnsw_idx
    ON ontology_entity_embedding USING hnsw (embedding vector_cosine_ops)
    WHERE embedding IS NOT NULL;


-- --- RLS -------------------------------------------------------------------

ALTER TABLE ontology_entity_search_vector ENABLE ROW LEVEL SECURITY;
ALTER TABLE ontology_entity_search_vector FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON ontology_entity_search_vector
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON ontology_entity_search_vector
    USING (current_setting('app.system_bypass', true) = 'true');

ALTER TABLE ontology_entity_embedding ENABLE ROW LEVEL SECURITY;
ALTER TABLE ontology_entity_embedding FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON ontology_entity_embedding
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON ontology_entity_embedding
    USING (current_setting('app.system_bypass', true) = 'true');

-- ============================================================================
-- Ontology facet indexes
-- ============================================================================

CREATE INDEX ontology_property_index_role_type_facet_idx
    ON ontology_property_index
       (version_id, aggregation_role, property_type);

-- PII surface: pick the PII-classified properties first, then
-- narrow by semantic_type (Email, Phone, Address, ...).
CREATE INDEX ontology_property_index_pii_semantic_facet_idx
    ON ontology_property_index
       (version_id, pii_kind, semantic_type)
    WHERE pii_kind IS NOT NULL;

-- Localisation surface — "all localized properties of kind T".
-- Partial index keeps size bounded.
CREATE INDEX ontology_property_index_localized_facet_idx
    ON ontology_property_index (version_id, property_type)
    WHERE is_localized = TRUE;


-- --- Link mapping facets ---------------------------------------------------

-- Correctness-critical for the compiler auto-DISTINCT path
-- (Π-2). "Every ManyToMany link of every kind in the current
-- version" is resolved in one seek so the compiler can inject
-- DISTINCT eagerly rather than per-edge at plan time.
CREATE INDEX ontology_link_mapping_index_cardinality_kind_facet_idx
    ON ontology_link_mapping_index
       (version_id, cardinality, kind);


-- --- Object mapping facets -------------------------------------------------

-- "All mappings in this version from a given source_id, ordered
-- by precedence" — the VOL planner's hot path for multi-mapping
-- dedup.
CREATE INDEX ontology_object_mapping_index_source_precedence_facet_idx
    ON ontology_object_mapping_index
       (version_id, source_id, precedence DESC);


-- --- Coded value facets ----------------------------------------------------

-- "All active codes in a system, by code" — value-semantics
-- prompt builder enumerates these in order.
CREATE INDEX ontology_coded_value_index_active_facet_idx
    ON ontology_coded_value_index
       (version_id, code_system_id, code)
    WHERE deprecated_at IS NULL;

-- ============================================================================
-- Ambiguity contexts + resolutions
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

-- ============================================================================
-- Change routing matrix + global seed
-- ============================================================================

CREATE TABLE change_routing_rules (
    id              UUID PRIMARY KEY,
    -- NULL = global default (shipped by this migration).
    -- Non-NULL = workspace override.
    workspace_id    UUID,
    change_type     TEXT NOT NULL
        CHECK (change_type IN (
            'coded_value_create',
            'coded_value_deprecate',
            'glossary_term_create',
            'glossary_alias_add',
            'notation_pattern_create',
            'customer_segment_create',
            'column_rename',
            'table_merge',
            'data_source_register',
            'stale_concept_deprecate',
            'ontology_version_rollback',
            'rule_create',
            'rule_modify',
            'rule_delete'
        )),
    routing         JSONB NOT NULL,
    risk_level      TEXT NOT NULL CHECK (risk_level IN ('low', 'medium', 'high')),
    priority        INT  NOT NULL DEFAULT 100,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),

    UNIQUE (workspace_id, change_type)
);

ALTER TABLE change_routing_rules ENABLE ROW LEVEL SECURITY;
ALTER TABLE change_routing_rules FORCE ROW LEVEL SECURITY;

-- Workspace-scoped rows follow standard RLS. Global rows
-- (`workspace_id IS NULL`) are readable by every workspace so the
-- policy splits into two:
-- - Workspace override visible when its workspace matches the session
-- - Global default visible to everyone (write gated separately)
CREATE POLICY ws_or_global_read ON change_routing_rules
    FOR SELECT
    USING (
        workspace_id IS NULL
        OR workspace_id = current_setting('app.workspace_id', true)::uuid
    );

-- Only a workspace can write its OWN overrides. Globals are
-- migration-managed (or system-bypass only).
CREATE POLICY ws_write ON change_routing_rules
    FOR ALL
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);

CREATE POLICY system_bypass ON change_routing_rules
    USING (current_setting('app.system_bypass', true) = 'true');

-- `priority DESC, workspace_id NULLS LAST` is the resolution order —
-- the index keeps the router's hot path fast at scale.
CREATE INDEX idx_change_routing_rules_resolve
    ON change_routing_rules (change_type, priority DESC)
    INCLUDE (workspace_id, routing);


-- Seed global defaults — one per ChangeType variant. Routing JSON is
-- authored to match the patent matrix and is kept in sync with the
-- Rust `ChangeType::default_routing` impl. The `risk_level` tier is a
-- UI/audit hint; it does NOT affect routing by itself.
INSERT INTO change_routing_rules (id, workspace_id, change_type, routing, risk_level, priority) VALUES
    (
        gen_random_uuid(), NULL, 'coded_value_create',
        '{"kind":"approval_required_unless","skip_predicates":[{"kind":"author_has_role","role":"data_steward"},{"kind":"change_scope_below","scope":"code_count","threshold":5}]}'::jsonb,
        'low', 0
    ),
    (
        gen_random_uuid(), NULL, 'coded_value_deprecate',
        '{"kind":"approval_required"}'::jsonb,
        'medium', 0
    ),
    (
        gen_random_uuid(), NULL, 'glossary_term_create',
        '{"kind":"approval_required_unless","skip_predicates":[{"kind":"author_has_role","role":"data_steward"}]}'::jsonb,
        'low', 0
    ),
    (
        gen_random_uuid(), NULL, 'glossary_alias_add',
        '{"kind":"approval_required_unless","skip_predicates":[{"kind":"author_has_role","role":"data_steward"}]}'::jsonb,
        'low', 0
    ),
    (
        gen_random_uuid(), NULL, 'notation_pattern_create',
        '{"kind":"approval_required"}'::jsonb,
        'medium', 0
    ),
    (
        gen_random_uuid(), NULL, 'customer_segment_create',
        '{"kind":"approval_required"}'::jsonb,
        'medium', 0
    ),
    (
        gen_random_uuid(), NULL, 'column_rename',
        '{"kind":"approval_required_unless","skip_predicates":[{"kind":"author_has_role","role":"admin"}]}'::jsonb,
        'high', 0
    ),
    (
        gen_random_uuid(), NULL, 'table_merge',
        '{"kind":"approval_required"}'::jsonb,
        'high', 0
    ),
    (
        gen_random_uuid(), NULL, 'data_source_register',
        '{"kind":"approval_required_unless","skip_predicates":[{"kind":"author_has_role","role":"admin"}]}'::jsonb,
        'medium', 0
    ),
    (
        gen_random_uuid(), NULL, 'stale_concept_deprecate',
        '{"kind":"auto_approve_with_notification","notify_roles":["data_steward"]}'::jsonb,
        'low', 0
    ),
    (
        gen_random_uuid(), NULL, 'ontology_version_rollback',
        '{"kind":"auto_approve"}'::jsonb,
        'medium', 0
    ),
    (
        gen_random_uuid(), NULL, 'rule_create',
        '{"kind":"approval_required_unless","skip_predicates":[{"kind":"author_has_role","role":"data_steward"}]}'::jsonb,
        'medium', 0
    ),
    (
        gen_random_uuid(), NULL, 'rule_modify',
        '{"kind":"approval_required_unless","skip_predicates":[{"kind":"author_has_role","role":"admin"}]}'::jsonb,
        'high', 0
    ),
    (
        gen_random_uuid(), NULL, 'rule_delete',
        '{"kind":"approval_required"}'::jsonb,
        'high', 0
    );

-- ============================================================================
-- Quality execution signals + last-used tracker
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

-- ============================================================================
-- Stale concept proposals
-- ============================================================================

CREATE TABLE stale_concept_proposals (
    id                    UUID PRIMARY KEY,
    workspace_id          UUID NOT NULL,
    type_id               UUID NOT NULL,
    type_kind             TEXT NOT NULL,
    last_used_at          TIMESTAMPTZ,
    days_since_last_use   INT NOT NULL,
    proposed_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    decision              TEXT NOT NULL DEFAULT 'pending'
        CHECK (decision IN ('pending', 'approved', 'dismissed')),
    decided_at            TIMESTAMPTZ,
    decided_by_user_id    UUID,
    reason                TEXT,

    UNIQUE (workspace_id, type_id)
);

ALTER TABLE stale_concept_proposals ENABLE ROW LEVEL SECURITY;
ALTER TABLE stale_concept_proposals FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON stale_concept_proposals
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON stale_concept_proposals
    USING (current_setting('app.system_bypass', true) = 'true');

-- List-open-proposals is the hot read path.
CREATE INDEX idx_stale_concept_proposals_open
    ON stale_concept_proposals (workspace_id, proposed_at DESC)
    WHERE decision = 'pending';

-- Federation plan-cache lookups by source id are the hot path for
-- `ontology_drafts`; the dedicated index keeps them O(log n). The
-- column itself is declared inline on the CREATE above.
CREATE INDEX ontology_drafts_source_id_idx ON ontology_drafts (source_id);

-- ============================================================================
-- Workspace quality baseline (adaptive thresholds)
-- ============================================================================

CREATE TABLE workspace_quality_baseline (
    workspace_id uuid PRIMARY KEY,

    -- Metric window the cron used to compute this snapshot ("7d" /
    -- "30d" / "90d"). Stored as text so new windows don't require a
    -- migration. `window_label` (not `window`) because `WINDOW` is
    -- a PostgreSQL reserved keyword and bare `window` trips the
    -- CREATE TABLE parser.
    window_label text NOT NULL DEFAULT '30d',

    -- Sample size that fed the computation. Frontend treats
    -- baselines with fewer than `MIN_SAMPLE_SIZE` signals as
    -- insufficient evidence and falls back to the hardcoded prior.
    sample_size bigint NOT NULL DEFAULT 0,

    -- Per-metric `{median, mad, warn, critical}` bundle. The cron
    -- computes `warn = median ± 2·MAD` and `critical = median ±
    -- 3·MAD` per metric key (shacl_pass_rate, query_reproducibility,
    -- anchor_match_rate, glossary_hit_rate, clarification_success_rate,
    -- stale_concept_ratio). JSONB rather than typed columns so new
    -- metrics land without a schema change — the banner's alert
    -- engine reads by key regardless.
    thresholds jsonb NOT NULL DEFAULT '{}'::jsonb,

    computed_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE workspace_quality_baseline ENABLE ROW LEVEL SECURITY;
ALTER TABLE workspace_quality_baseline FORCE ROW LEVEL SECURITY;

CREATE POLICY ws_isolation ON workspace_quality_baseline
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);

CREATE POLICY system_bypass ON workspace_quality_baseline
    USING (current_setting('app.system_bypass', true) = 'true')
    WITH CHECK (current_setting('app.system_bypass', true) = 'true');

