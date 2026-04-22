-- ============================================================================
-- 0023_drop_saved_ontologies.sql
--
-- Λ-13 — retire the legacy single-JSONB ontology table.
--
-- Pre-Λ, the authoritative ontology row lived in `saved_ontologies` and
-- dependent tables carried FKs into it. Phase 1/2 (migrations 0016-0022)
-- moved every caller onto `ontologies` + `ontology_version_snapshots` +
-- the content-addressed entity graph. No Rust code path writes to
-- `saved_ontologies` any more; migration 0022 already dropped the FKs
-- pointing at it.
--
-- This migration drops the table outright. The corresponding Rust
-- structures (SavedOntology model, OntologyStore trait + impl) are
-- removed in the same commit so a downgrade would break at the type
-- layer before it could re-reach the missing table.
-- ============================================================================

DROP TABLE IF EXISTS saved_ontologies CASCADE;
