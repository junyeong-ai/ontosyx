-- Knowledge entries hybrid retrieval indexes.
--
-- Adds the three lookup paths the RRF fusion consumes:
--
-- 1. HNSW on `embedding` — pgvector cosine NN. Already-populated
--    rows light up immediately; cold-start rows (embedding IS
--    NULL) silently fall out of the vector ranker.
-- 2. GIN trigram on `title` — typo / cosmetic recall on short
--    fields. The runtime questions the operator types into the
--    Brain are paraphrases of correction titles ("Use timestamp
--    truncation, not date casts"); trigram catches the close-by
--    surface forms before the embedding ranker sees them.
-- 3. GIN trigram on `content` — same shape on the longer prose.
--
-- The lexical FTS arm is already served by
-- `knowledge_entries_searchable_tsv` (GIN on the GENERATED
-- `searchable_tsv` from `tokenized_text`).
--
-- All three indexes are partial / conditional where it shrinks
-- index footprint without losing hits — the embedding HNSW is
-- bootstrap-aware (NULL rows excluded), the trigram indexes
-- stay full because every row carries title + content.

CREATE INDEX idx_knowledge_embedding
    ON knowledge_entries USING hnsw (embedding vector_cosine_ops)
    WHERE embedding IS NOT NULL;

CREATE INDEX idx_knowledge_title_trgm
    ON knowledge_entries USING gin (title gin_trgm_ops);

CREATE INDEX idx_knowledge_content_trgm
    ON knowledge_entries USING gin (content gin_trgm_ops);
