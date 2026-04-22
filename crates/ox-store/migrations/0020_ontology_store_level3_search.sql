-- ============================================================================
-- 0020_ontology_store_level3_search.sql
--
-- Λ Phase — Level 3 (C): Search + embedding indexes.
--
-- Two tables supporting entry-point discovery for the Progressive
-- Disclosure navigation API (Λ-11):
--
--   ontology_entity_search_vector   full-text + trigram fuzzy.
--                                    One row per (version, entity).
--                                    Backs `search_entry_points(q)`.
--   ontology_entity_embedding       semantic similarity (pgvector
--                                    HNSW). One row per (version,
--                                    entity). Backs `similar_to()`
--                                    and seeds the "LLM might mean
--                                    this concept" ranking.
--
-- Both extensions (`pg_trgm`, `vector`) are managed by the
-- docker-compose init (see 0001_schema.sql header). This
-- migration uses them directly.
-- ============================================================================


-- --- Full-text + trigram search -------------------------------------------
--
-- `doc` is the authored-language text the admin actually reads:
-- label / name / display_name / description / aliases /
-- scope_note — all concatenated and locale-flattened. The
-- populate-time Rust layer (Λ-10) flattens LocalizedText maps
-- into a single space-joined string.
--
-- `tsv` is the `to_tsvector('simple', doc)` of that text. We
-- use 'simple' (no stemming) rather than a language-specific
-- config because the admin may author in any mix of languages
-- and 'simple' preserves exact tokens for cross-locale search.
-- GIN index supports prefix + full-match + AND/OR queries.
--
-- The raw `doc` column stays so `pg_trgm` can score fuzzy
-- matches ("similarity(doc, 'orrder')" catches "order"). A
-- GIN `gin_trgm_ops` index on `doc` makes the similarity
-- search index-backed.

CREATE TABLE ontology_entity_search_vector (
    version_id    UUID NOT NULL
                       REFERENCES ontology_version_snapshots(id)
                       ON DELETE CASCADE,
    entity_kind   ontology_entity_kind NOT NULL,
    logical_id    TEXT NOT NULL,
    doc           TEXT NOT NULL,
    tsv           tsvector NOT NULL,
    workspace_id  UUID NOT NULL DEFAULT (current_setting('app.workspace_id', true))::uuid
                       REFERENCES workspaces(id) ON DELETE CASCADE,

    PRIMARY KEY (version_id, entity_kind, logical_id)
);

CREATE INDEX ontology_entity_search_vector_tsv_idx
    ON ontology_entity_search_vector USING gin (tsv);
CREATE INDEX ontology_entity_search_vector_trgm_idx
    ON ontology_entity_search_vector USING gin (doc gin_trgm_ops);


-- --- Semantic embedding ---------------------------------------------------
--
-- `embedding` is populated async by a batched background job
-- (the Gemini Embedding 2 API round-trip would be latency
-- prohibitive on the commit hot path). Rows exist here only
-- after a successful embedding fetch; callers that hit a cold
-- row fall back to full-text / trigram.
--
-- Dimension 1536 matches Gemini Embedding 2 Preview's MRL-
-- reduced output; HNSW index over `vector_cosine_ops` serves
-- top-K cosine-similarity queries in sub-20ms against the
-- Ontoserver-scale target (100K entities × 1536 dims).

CREATE TABLE ontology_entity_embedding (
    version_id    UUID NOT NULL
                       REFERENCES ontology_version_snapshots(id)
                       ON DELETE CASCADE,
    entity_kind   ontology_entity_kind NOT NULL,
    logical_id    TEXT NOT NULL,
    embedding     vector(1536),
    workspace_id  UUID NOT NULL DEFAULT (current_setting('app.workspace_id', true))::uuid
                       REFERENCES workspaces(id) ON DELETE CASCADE,

    PRIMARY KEY (version_id, entity_kind, logical_id)
);

-- HNSW index — accelerates `<=>` cosine-distance top-K lookups.
-- Partial index skips rows where `embedding IS NULL` (cold rows
-- haven't been populated yet), keeping the index small.
CREATE INDEX ontology_entity_embedding_hnsw_idx
    ON ontology_entity_embedding USING hnsw (embedding vector_cosine_ops)
    WHERE embedding IS NOT NULL;


-- --- RLS -------------------------------------------------------------------

ALTER TABLE ontology_entity_search_vector ENABLE ROW LEVEL SECURITY;
ALTER TABLE ontology_entity_search_vector FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON ontology_entity_search_vector
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON ontology_entity_search_vector
    USING (current_setting('app.system_bypass', true) = 'true');

ALTER TABLE ontology_entity_embedding ENABLE ROW LEVEL SECURITY;
ALTER TABLE ontology_entity_embedding FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON ontology_entity_embedding
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON ontology_entity_embedding
    USING (current_setting('app.system_bypass', true) = 'true');
