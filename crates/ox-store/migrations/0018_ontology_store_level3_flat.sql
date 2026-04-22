-- ============================================================================
-- 0018_ontology_store_level3_flat.sql
--
-- Λ Phase — Level 3 (A): per-kind flat indexes.
--
-- Materialised views of the Level 2 content-addressed entity store
-- as relational rows, one table per entity kind. The purpose is a
-- HOT-PATH query surface: "list all Property rows in version V
-- with aggregation_role = Measure" resolves via a single B-tree
-- index instead of scanning + JSONB-decoding every entity_versions
-- row for that version.
--
-- Populated by `commit_version` in the Rust layer (Λ-10); this
-- migration only defines the schema.
--
-- ## Coverage
--
-- 16 flat tables. Lower-cardinality entities (action, data_quality,
-- provenance, enrichment, rule, function, metric) stay Level-2-only
-- today — their query volume does not justify the materialisation
-- overhead. They can be promoted individually in a later migration
-- if query patterns shift.
--
-- Index columns per kind are the ones the navigation / admin UI /
-- LLM prompt layer actually FILTER + ORDER on:
--
--   node_type        : label, deprecated_at
--   edge_type        : label, source_type_id, target_type_id
--   property         : owner_kind, owner_logical_id, key, property_type,
--                      aggregation_role, value_set_id, notation_pattern_id,
--                      semantic_type, pii_kind, is_localized, deprecated_at
--   interface        : label
--   object_mapping   : node_type_id, source_id, precedence
--   link_mapping     : edge_type_id, kind, cardinality
--   property_mapping : property_id, source_id
--   code_system      : name, uri, kind, hierarchical
--   coded_value      : code_system_id, code, broader_id, deprecated_at
--   value_set        : name
--   notation_pattern : name
--   concept_map      : source_system_id, target_system_id
--   value_range_set  : name
--   glossary_term    : term, category, parent_term_id
--   rule             : kind
--   function         : name, purity
--
-- Every flat table carries `version_id` as part of the composite
-- PK so "load all Properties for version V" hits one btree-range
-- scan. When a version is deleted (rare — history prune), the
-- ON DELETE CASCADE on `version_id` drops the flat rows too.
--
-- ## Provenance
--
-- The `entity_hash` column on every table points back at
-- `ontology_entity_versions(entity_hash)`. Callers that need the
-- full content (beyond the facet columns) JOIN back to Level 2.
-- This keeps the flat tables cheap to write and cheap to query
-- while preserving the "single source of truth" property of
-- Level 2.
-- ============================================================================


-- --- node_type -------------------------------------------------------------

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
-- value_set_id, notation_pattern_id, semantic_type, pii_kind — so
-- the materialisation here lets the prompt-builder ask "show me all
-- Measure-role properties referencing value sets" in one index seek.

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
    value_set_id           TEXT,
    notation_pattern_id    TEXT,
    value_range_set_id     TEXT,
    semantic_type          TEXT,
    pii_kind               TEXT,
    unit_id                TEXT,
    glossary_term_id       TEXT,
    deprecated_at          TIMESTAMPTZ,
    workspace_id           UUID NOT NULL DEFAULT (current_setting('app.workspace_id', true))::uuid
                                REFERENCES workspaces(id) ON DELETE CASCADE,
    PRIMARY KEY (version_id, owner_kind, owner_logical_id, logical_id)
);
CREATE INDEX ontology_property_index_version_idx
    ON ontology_property_index (version_id);
CREATE INDEX ontology_property_index_value_set_idx
    ON ontology_property_index (version_id, value_set_id)
    WHERE value_set_id IS NOT NULL;
CREATE INDEX ontology_property_index_notation_pattern_idx
    ON ontology_property_index (version_id, notation_pattern_id)
    WHERE notation_pattern_id IS NOT NULL;


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
    parent_term_id    TEXT,
    workspace_id      UUID NOT NULL DEFAULT (current_setting('app.workspace_id', true))::uuid
                           REFERENCES workspaces(id) ON DELETE CASCADE,
    PRIMARY KEY (version_id, logical_id)
);
CREATE INDEX ontology_glossary_term_index_term_idx
    ON ontology_glossary_term_index (version_id, term);
CREATE INDEX ontology_glossary_term_index_parent_idx
    ON ontology_glossary_term_index (version_id, parent_term_id)
    WHERE parent_term_id IS NOT NULL;


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
