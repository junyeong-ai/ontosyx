-- ============================================================================
-- 0025_change_routing.sql
--
-- Change-type routing rules. One row per
-- `(workspace_id?, change_type)`; the router reads them ordered by
-- `priority DESC` to let per-workspace overrides win against the
-- global defaults.
--
-- The migration seeds one global default rule per `ChangeType`
-- variant using the patent matrix's automation tier. Workspaces
-- install overrides by INSERTing rows with the same `change_type`
-- and a higher `priority`.
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
            'ontology_version_rollback'
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
        '{"kind":"approval_required_unless","skip_predicates":[{"kind":"author_has_role","role":"data_steward"},{"kind":"change_scope_below","code_count_delta":5}]}'::jsonb,
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
        '{"kind":"approval_required_unless","skip_predicates":[{"kind":"author_has_role","role":"admin"},{"kind":"has_validation_pass"}]}'::jsonb,
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
    );
