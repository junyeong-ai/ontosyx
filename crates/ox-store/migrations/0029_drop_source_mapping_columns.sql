-- Legacy `source_mapping` JSONB column drop. Post-refactor the
-- canonical ObjectMappingDef list lives inside OntologyIR
-- (`ontology.object_mappings`), which travels through the existing
-- `ontology` column on both design_projects and ontology_snapshots.
-- The separate blob was a Phase 4-A holdover; keeping it would mean
-- two persistence paths for the same information with no source of
-- truth.
--
-- No backfill needed: callers either never populated this field or
-- did so alongside `ontology`, which remains authoritative.

ALTER TABLE design_projects DROP COLUMN source_mapping;
ALTER TABLE ontology_snapshots DROP COLUMN source_mapping;
