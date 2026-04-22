-- ============================================================================
-- 0019_ontology_store_level3_navigation.sql
--
-- Λ Phase — Level 3 (B): Navigation indexes.
--
-- Two tables, both populated by `commit_version`:
--
--   ontology_entity_neighbors    1-hop directed graph — enables
--                                "from entity X, what are its direct
--                                 references and what are the direct
--                                 back-references".
--   ontology_entity_hierarchy    Transitive closure over hierarchical
--                                relations (Interface implements,
--                                GlossaryTerm parent_term, CodedValue
--                                broader_id) — O(1) ancestor /
--                                descendant lookup at any depth.
--
-- Closure tables are the classic materialisation technique for
-- hierarchical traversal on PostgreSQL; they pay write-time for
-- O(1) reads at any depth, matching our workload (write is rare,
-- LLM prompt / navigation read is hot).
--
-- ## Semantics
--
-- 1-hop `ontology_entity_neighbors` rows model a directed edge
-- `(from_kind, from_logical_id) --relation_kind--> (to_kind,
-- to_logical_id)`. Examples:
--   - (property, prop-x) --references_value_set--> (value_set, vs-y)
--   - (property, prop-x) --references_notation--> (notation_pattern, np-y)
--   - (property, prop-x) --uses_unit--> (coded_value, ucum-kg)
--   - (object_mapping, om-z) --maps_node_type--> (node_type, nt-y)
--   - (link_mapping, lm-z) --maps_edge_type--> (edge_type, et-y)
--   - (concept_map, cm-z) --source--> (code_system, cs-a)
--   - (concept_map, cm-z) --target--> (code_system, cs-b)
--
-- Closure rows (`entity_hierarchy`) model transitive `broader /
-- parent` relations: the table stores `(ancestor, descendant,
-- depth)` for every ancestor of every hierarchical entity. Self-
-- links at depth 0 are included so a single read returns the
-- entity plus its entire ancestor chain.
--
-- Relation kinds are free-form TEXT (enum-ish) rather than an
-- ENUM type so new cross-references — added when the IR grows a
-- new Ω/Π axis — don't require an ALTER TYPE migration. Typos
-- surface at the populate-time Rust layer where strings are
-- constants.
-- ============================================================================


-- --- 1-hop neighbor graph --------------------------------------------------

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
--   glossary_term_parent      GlossaryTermDef.parent_term_id chain
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
