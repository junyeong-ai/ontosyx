-- Φ12.1 — SourceContractDef persistence
--
-- Frozen physical-shape snapshot of a source relation captured at
-- introspection time. Consumed by the commit-path validator
-- (`OntologyIR::validate_against_source_contracts`) to enforce
-- mapping ↔ source fidelity before a workspace's canonical
-- ontology can advance.
--
-- 4-clause RLS, workspace-scoped. UPSERT on (workspace_id,
-- source_id, relation) — idempotent re-introspection.

CREATE TABLE source_contracts (
    workspace_id     UUID            NOT NULL,
    source_id        TEXT            NOT NULL,
    relation         TEXT            NOT NULL,
    columns          JSONB           NOT NULL,
    primary_key      JSONB           NOT NULL DEFAULT '[]'::jsonb,
    fingerprint      TEXT            NOT NULL,
    introspected_at  TIMESTAMPTZ     NOT NULL DEFAULT now(),

    PRIMARY KEY (workspace_id, source_id, relation),

    CONSTRAINT source_contracts_source_id_non_empty
        CHECK (length(btrim(source_id)) > 0),
    CONSTRAINT source_contracts_relation_non_empty
        CHECK (length(btrim(relation)) > 0),
    CONSTRAINT source_contracts_columns_array
        CHECK (jsonb_typeof(columns) = 'array'),
    CONSTRAINT source_contracts_columns_non_empty
        CHECK (jsonb_array_length(columns) > 0),
    CONSTRAINT source_contracts_primary_key_array
        CHECK (jsonb_typeof(primary_key) = 'array'),
    CONSTRAINT source_contracts_fingerprint_non_empty
        CHECK (length(fingerprint) > 0)
);

CREATE INDEX source_contracts_workspace_source
    ON source_contracts (workspace_id, source_id);

ALTER TABLE source_contracts ENABLE ROW LEVEL SECURITY;
ALTER TABLE source_contracts FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON source_contracts
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON source_contracts
    USING (current_setting('app.system_bypass', true) = 'true');
