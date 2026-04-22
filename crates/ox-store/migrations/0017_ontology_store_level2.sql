-- ============================================================================
-- 0017_ontology_store_level2.sql
--
-- Λ Phase — Level 2: Content-Addressed Entity Store.
--
-- Level 1 (0016) gave each ontology version an identity row but
-- no content. Level 2 stores the actual entity data, keyed by the
-- SHA-256 hash of the entity's canonical JSON — the "content
-- address" model proven by Git, Dolt, IPLD, and Nessie.
--
-- ## Core invariants
--
-- 1. **Immutability**. Rows in `ontology_entity_versions` are
--    INSERT-only. UPDATE is a contract violation: the row's key
--    *is* its content-hash, so a mutation would invalidate the
--    key itself. The schema has no UPDATE grant; the caller path
--    uses INSERT ... ON CONFLICT (entity_hash) DO NOTHING for
--    idempotent upserts.
--
-- 2. **Cross-version dedup**. When version N+1 leaves an entity
--    unchanged, its hash is identical to version N's, and the
--    new version's pointer set (`ontology_version_entities`)
--    simply points at the existing row. No storage cost per
--    unchanged entity across versions.
--
-- 3. **Stable logical identity**. An entity's `logical_id` is
--    the stable identifier authors reference (`NodeTypeId`,
--    `EdgeTypeId`, `CodeSystemId`, etc.). When the entity's
--    content changes, its `entity_hash` changes but the
--    `logical_id` stays. This is how rename / field-edit tracking
--    works across versions — walk the pointer chain by
--    `(kind, logical_id)` and the hash history surfaces the
--    edits.
--
-- 4. **Kind taxonomy**. One row per top-level collection in the
--    `OntologyIR` + an `ontology_header` pseudo-kind for the
--    ontology's own name/description/version metadata. Nested
--    entities (property inside node_type, coded_value inside
--    code_system, constraint inside node_type) ride along with
--    their parent in the `content` JSONB. The parent-level hash
--    changes when any nested field changes, which is correct:
--    "NodeType as a whole changed" is the edit granularity that
--    matters for dedup and diff.
--
-- ## Scope
--
-- This migration creates the two Level 2 tables. The Rust-side
-- entity extractor (Λ-3) + commit logic (Λ-4/Λ-5) lands next.
-- Level 3 materialised indexes (Λ-6-Λ-9) are derived from this
-- store.
-- ============================================================================

-- --- Entity kind taxonomy --------------------------------------------------
--
-- Enumerates every kind of row the content-addressed store can
-- hold. Matches the 6-axis IR model from §5:
--
--   Axis ①  Topology               node_type, edge_type, index_def, interface
--   Axis ②  Physical mapping       object_mapping, link_mapping, property_mapping
--   Axis ③  Governance             rule, data_quality, action, provenance
--   Axis ④  Behaviour              function, metric, enrichment
--   Axis ⑤  Vocabulary + values    glossary_term, taxonomy, code_system,
--                                   value_set, notation_pattern, concept_map,
--                                   value_range_set
--   header  (axis-6 metadata)      ontology_header
--
-- The enum is a hard-coded PostgreSQL type. Adding a kind
-- requires a migration that runs `ALTER TYPE ... ADD VALUE`
-- (Postgres supports it without a table rewrite). The Rust enum
-- that mirrors this must stay in lockstep — drift is caught at
-- the hydration layer when an unknown variant arrives from the
-- DB.

CREATE TYPE ontology_entity_kind AS ENUM (
    'ontology_header',
    -- Topology
    'node_type',
    'edge_type',
    'index_def',
    'interface',
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
    'value_set',
    'notation_pattern',
    'concept_map',
    'value_range_set'
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
