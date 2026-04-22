-- ============================================================================
-- 0016_ontology_store_level1.sql
--
-- Λ Phase — Enterprise Storage Refactor — Level 1: Identity & Versioning.
--
-- Replaces the single-JSONB-blob `saved_ontologies` table with a
-- four-level storage model (this migration is Level 1; Levels 2-4
-- follow in 0017-0021). The full design is in the v2 Confluence
-- doc §8; at a glance:
--
--   Level 1  (this file)  identity + version snapshots (pointer set)
--   Level 2  (0017)       content-addressed entity store (immutable)
--   Level 3  (0018-0021)  materialised indexes for hot-path queries
--   Level 4  (existing)   workspaces · users · sessions · audit
--
-- Rationale for the refactor:
-- - Single-blob storage re-serialises the full IR on every tiny edit
--   (MB-scale writes per field change).
-- - Diffing two versions requires full-tree JSONB comparison.
-- - Enterprise-scale ontologies (10K node types, 100K properties,
--   1M coded values) blow out both storage cost and query latency.
-- - No entity-level dedup across versions.
--
-- The new model matches the structural fusion adopted by Palantir
-- Foundry Ontology (entity-level rows), Dolt + IPLD + Git
-- (content-addressed canonical hashing), and HL7 FHIR Terminology
-- Server (version snapshot + entity reuse).
--
-- ## Migration strategy — coexistence
--
-- `saved_ontologies` is NOT dropped in this migration. Λ-12
-- migrates all callers (ox-api routes + ox-brain + ox-agent) off
-- the legacy `OntologyStore` onto `OntologyVersionStore`; Λ-13
-- then drops `saved_ontologies` in a final migration once every
-- caller is off it. Attempting to drop it here would leave the
-- server unbootable mid-refactor (every `saved_ontologies`
-- reference from the Rust layer would 42P01 until Λ-12 landed).
--
-- The new Level 1/2/3 tables below live alongside the legacy
-- table during the transition. Both systems are write-able, but
-- Rust code paths choose ONE or the OTHER — the two are never
-- kept in sync. Dropping `saved_ontologies` in Λ-13 is the
-- explicit "legacy has no callers" signal.
-- ============================================================================


-- --- Level 1 : ontologies --------------------------------------------------
--
-- One row per logical ontology (distinct from a version). A single
-- ontology evolves through many `ontology_version_snapshots` rows;
-- all of them share the same `lineage_id` so quality rules,
-- saved queries, and external mappings that reference the
-- "ontology" as a concept stay stable across version bumps.

CREATE TABLE ontologies (
    id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    -- Stable identity across versions. External systems (quality
    -- rules, saved queries, design projects) reference an ontology
    -- via lineage_id; two ontologies with the same `name` in
    -- different workspaces are still distinct because of RLS +
    -- `workspace_id`.
    -- TEXT rather than UUID because the ontology's `lineage_id`
    -- mirrors `OntologyIR.lineage_id` which is a `String` (see
    -- 0009_ontology_lineage_id.sql); LLM-generated lineage tags
    -- and imported ontologies may carry non-UUID identifiers.
    lineage_id     TEXT NOT NULL,
    workspace_id   UUID NOT NULL DEFAULT (current_setting('app.workspace_id', true))::uuid
                        REFERENCES workspaces(id) ON DELETE CASCADE,
    -- Canonical short name (`"E-commerce"`, `"Healthcare"`). Used
    -- for listing / search; not required to be Cypher-safe because
    -- the ontology itself is not a graph label.
    name           TEXT NOT NULL,
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
    CONSTRAINT ontologies_ws_name_uq UNIQUE (workspace_id, name)
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
