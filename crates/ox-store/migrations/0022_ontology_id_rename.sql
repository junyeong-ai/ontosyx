-- ============================================================================
-- 0022_ontology_id_rename.sql
--
-- Λ-12 Phase 2 — column rename + FK retarget.
--
-- `design_projects.saved_ontology_id` and `query_executions.saved_ontology_id`
-- used to FK into the legacy single-JSONB `saved_ontologies` table. With the
-- Λ-refactor the logical identity lives in `ontologies` (migration 0016);
-- `saved_ontologies` stays alive until Λ-13 drops it but is no longer the
-- authority for project/query references.
--
-- This migration:
--   1. Drops the FKs pointing at `saved_ontologies`.
--   2. Renames the columns to `ontology_id` — the semantic handle is now the
--      identity uuid in `ontologies`, and the `saved_` prefix was encoding
--      a storage detail rather than a domain concept.
--   3. NULLs existing values, because old saved_ontologies.id and new
--      ontologies.id are distinct keyspaces and the pre-refactor rows have
--      no mapping. Re-linking happens when callers re-save a project.
--   4. Renames supporting indexes + check constraints.
--   5. Adds new FKs pointing at `ontologies(workspace_id, id)` so workspace
--      isolation + ON DELETE semantics stay intact.
--
-- No BC shim — the legacy caller slice that still writes
-- `saved_ontologies.id` into these columns is either already migrated
-- (Phase 1) or is migrated in the same commit that applies this file.
-- ============================================================================


-- --- ontologies composite uniqueness ---------------------------------------
--
-- `ontologies.id` is PK (unique), but the new FKs below reference the
-- composite `(workspace_id, id)` to mirror the legacy pattern (workspace
-- scope pinned at the FK so a ws_isolation RLS violation can't slip
-- between the workspace scope check and the referential check). Add a
-- dedicated unique constraint on the pair BEFORE the FKs so PostgreSQL
-- can resolve them. (A fresh-DB run bit this before: the UNIQUE was at
-- the tail of the file and every FK above it failed with "there is no
-- unique constraint matching given keys for referenced table".)

ALTER TABLE ontologies
    ADD CONSTRAINT ontologies_ws_id_uq UNIQUE (workspace_id, id);


-- --- design_projects --------------------------------------------------------

ALTER TABLE design_projects
    DROP CONSTRAINT IF EXISTS design_projects_saved_ontology_ws_fk;

UPDATE design_projects SET saved_ontology_id = NULL;

ALTER TABLE design_projects
    RENAME COLUMN saved_ontology_id TO ontology_id;

ALTER TABLE design_projects
    ADD CONSTRAINT design_projects_ontology_ws_fk
        FOREIGN KEY (workspace_id, ontology_id)
        REFERENCES ontologies(workspace_id, id)
        ON DELETE SET NULL;


-- --- query_executions -------------------------------------------------------

ALTER TABLE query_executions
    DROP CONSTRAINT IF EXISTS query_executions_saved_ontology_ws_fk;

-- The pre-existing check constraint pins "either saved_ontology_id or
-- ontology_snapshot must be present". Same invariant applies post-rename;
-- we drop + recreate against the new column name.
ALTER TABLE query_executions
    DROP CONSTRAINT IF EXISTS chk_ontology_source;

UPDATE query_executions SET saved_ontology_id = NULL
 WHERE ontology_snapshot IS NOT NULL;

-- Rows with NULL snapshot AND non-null saved_ontology_id cannot satisfy the
-- new invariant once we NULL the legacy FK values; they pre-date the
-- refactor and have no equivalent under the new identity key. Drop them —
-- they'd fail the CHECK on the first write anyway.
DELETE FROM query_executions
 WHERE ontology_snapshot IS NULL
   AND saved_ontology_id IS NOT NULL;

ALTER TABLE query_executions
    RENAME COLUMN saved_ontology_id TO ontology_id;

ALTER TABLE query_executions
    ADD CONSTRAINT chk_ontology_source
        CHECK ((ontology_id IS NOT NULL) OR (ontology_snapshot IS NOT NULL));

ALTER TABLE query_executions
    ADD CONSTRAINT query_executions_ontology_ws_fk
        FOREIGN KEY (workspace_id, ontology_id)
        REFERENCES ontologies(workspace_id, id)
        ON DELETE RESTRICT;


-- --- indexes ---------------------------------------------------------------

DROP INDEX IF EXISTS idx_query_executions_ontology_ref;
CREATE INDEX idx_query_executions_ontology_id
    ON query_executions USING btree (ontology_id)
    WHERE ontology_id IS NOT NULL;
