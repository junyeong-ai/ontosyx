-- Extensions (pg_trgm, vector) are managed by docker-compose init.

-- ============================================================================
-- 1. Core
-- ============================================================================

CREATE TABLE users (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    email text NOT NULL,
    name text,
    picture text,
    provider text NOT NULL,
    provider_sub text NOT NULL,
    role text DEFAULT 'designer' NOT NULL,
    created_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    last_login_at TIMESTAMPTZ,
    CONSTRAINT users_pkey PRIMARY KEY (id),
    CONSTRAINT users_email_key UNIQUE (email),
    CONSTRAINT users_provider_provider_sub_key UNIQUE (provider, provider_sub)
);

CREATE TABLE workspaces (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    name VARCHAR(255) NOT NULL,
    slug VARCHAR(100) NOT NULL,
    owner_id uuid NOT NULL,
    settings jsonb DEFAULT '{}' NOT NULL,
    created_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    CONSTRAINT workspaces_pkey PRIMARY KEY (id),
    CONSTRAINT workspaces_slug_key UNIQUE (slug)
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

CREATE TABLE design_projects (
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
    source_mapping jsonb,
    ontology jsonb,
    quality_report jsonb,
    saved_ontology_id uuid,
    created_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    updated_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    analyzed_at TIMESTAMPTZ,
    archived_at TIMESTAMPTZ,
    workspace_id uuid DEFAULT (current_setting('app.workspace_id', true))::uuid NOT NULL,
    CONSTRAINT design_projects_pkey PRIMARY KEY (id)
);
ALTER TABLE ONLY design_projects FORCE ROW LEVEL SECURITY;

CREATE TABLE ontology_snapshots (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    project_id uuid NOT NULL,
    revision integer NOT NULL,
    ontology jsonb NOT NULL,
    source_mapping jsonb,
    quality_report jsonb,
    created_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    workspace_id uuid DEFAULT (current_setting('app.workspace_id', true))::uuid NOT NULL,
    CONSTRAINT ontology_snapshots_pkey PRIMARY KEY (id),
    CONSTRAINT ontology_snapshots_project_id_revision_key UNIQUE (project_id, revision)
);
ALTER TABLE ONLY ontology_snapshots FORCE ROW LEVEL SECURITY;

CREATE TABLE saved_ontologies (
    id uuid NOT NULL,
    name text NOT NULL,
    description text,
    version integer NOT NULL,
    ontology_ir jsonb NOT NULL,
    created_by text NOT NULL,
    created_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    workspace_id uuid DEFAULT (current_setting('app.workspace_id', true))::uuid NOT NULL,
    CONSTRAINT saved_ontologies_pkey PRIMARY KEY (id),
    CONSTRAINT saved_ontologies_name_version_key UNIQUE (name, version)
);
ALTER TABLE ONLY saved_ontologies FORCE ROW LEVEL SECURITY;

-- ============================================================================
-- 3. Query
-- ============================================================================

CREATE TABLE query_executions (
    id uuid NOT NULL,
    user_id text NOT NULL,
    question text NOT NULL,
    ontology_id text NOT NULL,
    ontology_version integer NOT NULL,
    saved_ontology_id uuid,
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
    CONSTRAINT chk_ontology_source CHECK ((saved_ontology_id IS NOT NULL) OR (ontology_snapshot IS NOT NULL))
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

-- ============================================================================
-- 4. Agent
-- ============================================================================

CREATE TABLE agent_sessions (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    ontology_id VARCHAR(255),
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
    project_id uuid,
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
    project_id uuid NOT NULL,
    source_table VARCHAR(255) NOT NULL,
    graph_label VARCHAR(255) NOT NULL,
    watermark_column VARCHAR(255) NOT NULL,
    watermark_value text NOT NULL,
    record_count bigint DEFAULT 0 NOT NULL,
    loaded_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    CONSTRAINT load_checkpoints_pkey PRIMARY KEY (id),
    CONSTRAINT load_checkpoints_workspace_id_project_id_source_table_graph_key UNIQUE (workspace_id, project_id, source_table, graph_label)
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
    ontology_id VARCHAR(255),
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
    ontology_id VARCHAR(255),
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
    CONSTRAINT prompt_templates_name_version_key UNIQUE (name, version)
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
    review_notes text,
    reviewed_at TIMESTAMPTZ,
    expires_at TIMESTAMPTZ DEFAULT (now() + '7 days'::interval) NOT NULL,
    created_at TIMESTAMPTZ DEFAULT now() NOT NULL,
    CONSTRAINT approval_requests_pkey PRIMARY KEY (id),
    CONSTRAINT approval_requests_status_check CHECK (status = ANY (ARRAY['pending', 'approved', 'rejected', 'expired']))
);
ALTER TABLE ONLY approval_requests FORCE ROW LEVEL SECURITY;

CREATE TABLE audit_log (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    user_id uuid,
    workspace_id uuid DEFAULT (current_setting('app.workspace_id', true))::uuid NOT NULL,
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
    ontology_id VARCHAR(255) NOT NULL,
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
    ontology_id VARCHAR(255) NOT NULL,
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
    project_id uuid,
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
