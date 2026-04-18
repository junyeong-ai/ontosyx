-- Migration 0009 — Ontology identity naming clarity + QualityRule scoping
--
-- The codebase had three overlapping "ontology id" concepts:
--
--   1. `OntologyIR.id: String`     — the semantic lineage identifier (stable
--                                     across revisions of the same ontology)
--   2. `SavedOntology.id: Uuid`    — the DB row id for a pinned version
--                                     snapshot (immutable per insert)
--   3. `KnowledgeEntry.ontology_name` — a human display name
--
-- Different tables stored (1) under the same column name `ontology_id`
-- while (2) lived as `saved_ontology_id`. The shared name for two
-- distinct concepts hurt reviewers and invited misuse. This migration
-- renames every occurrence of (1) to `ontology_lineage_id` so the
-- semantics match the column name, leaves (2) unchanged, and adds the
-- missing lineage scoping to `quality_rules` (previously workspace-
-- scoped only, which collapsed ontologies that shared a label).
--
-- The rename is lossless: PostgreSQL updates existing indexes and
-- constraints to reference the new column name automatically. No data
-- migration is needed since the underlying value is unchanged.

BEGIN;

-- ============================================================================
-- 1. Rename `ontology_id` → `ontology_lineage_id` across tables whose column
--    held a lineage string (OntologyIR.id), not a SavedOntology row id.
-- ============================================================================

ALTER TABLE query_executions        RENAME COLUMN ontology_id TO ontology_lineage_id;
ALTER TABLE agent_sessions          RENAME COLUMN ontology_id TO ontology_lineage_id;
ALTER TABLE analysis_results        RENAME COLUMN ontology_id TO ontology_lineage_id;
ALTER TABLE scheduled_tasks         RENAME COLUMN ontology_id TO ontology_lineage_id;
ALTER TABLE ontology_verifications  RENAME COLUMN ontology_id TO ontology_lineage_id;
ALTER TABLE saved_reports           RENAME COLUMN ontology_id TO ontology_lineage_id;
ALTER TABLE saved_query_patterns    RENAME COLUMN ontology_id TO ontology_lineage_id;

-- ============================================================================
-- 2. QualityRule: add the previously-missing ontology lineage reference.
--
-- Rules were workspace-scoped only, so two ontologies in the same workspace
-- that shared a node label (e.g., both have `Person`) would see their rules
-- collapse into one set. Binding each rule to a lineage keeps them distinct.
--
-- Existing rows get the empty string; the quality dashboard will treat an
-- empty lineage as "legacy, unscoped" and the UI prompts the author to
-- re-assign on next edit. A NOT NULL default avoids a second migration
-- round-trip for rows that exist today.
-- ============================================================================

ALTER TABLE quality_rules
    ADD COLUMN ontology_lineage_id text NOT NULL DEFAULT '';

-- Drop the default after backfill — new rows must supply the lineage.
ALTER TABLE quality_rules
    ALTER COLUMN ontology_lineage_id DROP DEFAULT;

-- Composite index on the lookup shape used by the evaluator sweep
-- (workspace_id comes from RLS predicate, so the index only needs the
-- fields the evaluator filters on).
CREATE INDEX idx_quality_rules_lineage
    ON quality_rules (ontology_lineage_id) WHERE is_active = true;

-- ============================================================================
-- 3. Rename legacy index identifiers so their names reflect the new
--    column semantics. Index definitions auto-update during the RENAME
--    COLUMN above; this rename is cosmetic so future maintainers don't
--    have to cross-reference old column names.
-- ============================================================================

ALTER INDEX IF EXISTS idx_saved_reports_ontology
    RENAME TO idx_saved_reports_lineage;

ALTER INDEX IF EXISTS idx_verifications_ontology
    RENAME TO idx_verifications_lineage;

ALTER INDEX IF EXISTS idx_saved_query_patterns_user_ontology
    RENAME TO idx_saved_query_patterns_user_lineage;

-- ============================================================================
-- 4. memory_entries.metadata JSONB key rename — ontology_id → ontology_lineage_id
--
-- MemoryMetadata is persisted as JSONB. Keep the wire shape in sync with
-- the renamed Rust struct field so existing rows keep matching filters.
-- The index over the JSONB path is dropped + recreated under the new key.
-- ============================================================================

DROP INDEX IF EXISTS idx_memory_metadata_ontology;

UPDATE memory_entries
SET metadata = (metadata - 'ontology_id')
             || jsonb_build_object('ontology_lineage_id', metadata->'ontology_id')
WHERE metadata ? 'ontology_id';

CREATE INDEX idx_memory_metadata_lineage
    ON memory_entries USING btree ((metadata ->> 'ontology_lineage_id'));

COMMIT;
