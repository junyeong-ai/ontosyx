-- 0012_search_vector_label.sql
--
-- Adds a separate `label` column to `ontology_entity_search_vector`
-- so the retrieval blend can score label-match independently from
-- description-match. Without this, `Customer` (a NodeType label)
-- and `gt_return` (a glossary term whose description contains
-- "customer") tied on doc-trigram similarity for the query
-- "customer", letting the description-heavy row outrank the
-- structural one.
--
-- Backfill is empty — the materialiser writes the column on every
-- subsequent commit. Existing rows leave `label` as `NULL`; the
-- blend SQL `COALESCE(label, '')` keeps them scored on doc-only,
-- preserving behaviour for rows from prior commits until they're
-- re-materialised.
--
-- The trigram index covers `label` so the prefix-match probe in
-- the new blend stays sub-millisecond on Ontoserver-scale tables.

ALTER TABLE ontology_entity_search_vector
    ADD COLUMN IF NOT EXISTS label TEXT;

CREATE INDEX IF NOT EXISTS ontology_entity_search_vector_label_trgm_idx
    ON ontology_entity_search_vector USING gin (label gin_trgm_ops);
