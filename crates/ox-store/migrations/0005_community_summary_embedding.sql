-- Community-summary embedding column for hybrid retrieval.
--
-- The 3-ranker RRF fusion the Brain consumes pulls candidates
-- from trigram (title + summary), tokenized FTS
-- (`searchable_tsv`), and pgvector cosine NN — this column adds
-- the third arm. Cold-start rows (embedding IS NULL) silently
-- drop out of the vector ranker; the partial HNSW index keeps
-- size bounded while the bank fills.
--
-- Dimension 1024 mirrors the workspace's default multilingual
-- embedding model — same target the verified-query bank pins.

ALTER TABLE ontology_community_summaries
    ADD COLUMN embedding vector(1024);

CREATE INDEX ontology_community_summaries_embedding_hnsw_idx
    ON ontology_community_summaries USING hnsw (embedding vector_cosine_ops)
    WHERE embedding IS NOT NULL;
