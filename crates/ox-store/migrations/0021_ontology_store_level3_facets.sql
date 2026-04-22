-- ============================================================================
-- 0021_ontology_store_level3_facets.sql
--
-- Λ Phase — Level 3 (D): Facet composite B-tree indexes.
--
-- Level 3 (A) (migration 0018) created per-column indexes tuned
-- for single-facet lookups. This migration adds COMPOSITE B-tree
-- indexes tuned for multi-facet queries the navigation API and
-- LLM prompt builder issue most often — the shape where one
-- index seek returns the exact shortlist instead of intersecting
-- two single-column indexes.
--
-- Queries this optimises:
--
--   "all Measure-role numeric Properties with a value_set"
--     → (version_id, aggregation_role, property_type, value_set_id)
--
--   "all PII Properties of a given kind"
--     → (version_id, pii_kind, semantic_type)
--
--   "all ManyToMany link mappings of a given kind"
--     → (version_id, cardinality, kind)
--
--   "sort OrderStatus codes by alias hits"
--     handled by the search vector + trgm index in 0020.
--
-- ## Why composite indexes now?
--
-- The single-column indexes in 0018 cover "all Measure-role
-- properties" but not the intersection "Measure × Int × with
-- value_set". Two-index bitmap intersection works in the
-- single-digit-thousand range but loses to a covering composite
-- when cardinality moves into the tens-of-thousands — which is
-- the enterprise-scale target.
-- ============================================================================


-- --- Property facets -------------------------------------------------------
--
-- Order (aggregation_role, property_type, value_set_id): role is
-- the most selective first column (4 values), then type, then
-- value_set linkage. Handles "show me all Measures that are Int
-- and have a value_set" in one B-tree seek.

CREATE INDEX ontology_property_index_role_type_vs_facet_idx
    ON ontology_property_index
       (version_id, aggregation_role, property_type, value_set_id);

-- PII surface: pick the PII-classified properties first, then
-- narrow by semantic_type (Email, Phone, Address, ...).
CREATE INDEX ontology_property_index_pii_semantic_facet_idx
    ON ontology_property_index
       (version_id, pii_kind, semantic_type)
    WHERE pii_kind IS NOT NULL;

-- Localisation surface — "all localized properties of kind T".
-- Partial index keeps size bounded.
CREATE INDEX ontology_property_index_localized_facet_idx
    ON ontology_property_index (version_id, property_type)
    WHERE is_localized = TRUE;


-- --- Link mapping facets ---------------------------------------------------

-- Correctness-critical for the compiler auto-DISTINCT path
-- (Π-2). "Every ManyToMany link of every kind in the current
-- version" is resolved in one seek so the compiler can inject
-- DISTINCT eagerly rather than per-edge at plan time.
CREATE INDEX ontology_link_mapping_index_cardinality_kind_facet_idx
    ON ontology_link_mapping_index
       (version_id, cardinality, kind);


-- --- Object mapping facets -------------------------------------------------

-- "All mappings in this version from a given source_id, ordered
-- by precedence" — the VOL planner's hot path for multi-mapping
-- dedup.
CREATE INDEX ontology_object_mapping_index_source_precedence_facet_idx
    ON ontology_object_mapping_index
       (version_id, source_id, precedence DESC);


-- --- Coded value facets ----------------------------------------------------

-- "All active codes in a system, by code" — value-semantics
-- prompt builder enumerates these in order.
CREATE INDEX ontology_coded_value_index_active_facet_idx
    ON ontology_coded_value_index
       (version_id, code_system_id, code)
    WHERE deprecated_at IS NULL;
