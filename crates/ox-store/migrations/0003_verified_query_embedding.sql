-- Φ11.5 — embedding column on verified_queries for semantic NN
-- retrieval.
--
-- The Φ11.2b trigram retriever stays — admin browse / filter and
-- the no-embedder-attached fallback path lean on it. The new
-- column lights up *semantic* top-k that lifts paraphrase-heavy
-- recall (the trigram ranker scores low when the user phrases the
-- same intent with different vocabulary).
--
-- Dimension 1024 mirrors the workspace's default multilingual
-- embedding model. Rows persisted before the embedder was attached
-- (or before this migration) have `embedding IS NULL` and the
-- partial HNSW index skips them — Vanna.AI's "cold start" path
-- where the bank fills before the embedder is hot.

ALTER TABLE verified_queries
    ADD COLUMN embedding vector(1024);

-- HNSW partial index — accelerates `<=>` cosine top-K lookups.
-- `WHERE embedding IS NOT NULL` keeps the index small while the
-- bank is bootstrapping; rows fill in lazily as the embedder
-- catches up via re-promote / explicit reindex.
CREATE INDEX verified_queries_embedding_hnsw_idx
    ON verified_queries USING hnsw (embedding vector_cosine_ops)
    WHERE embedding IS NOT NULL;
