-- 0010_evaluation_case_metadata.sql
--
-- `evaluation_cases.metadata: JSONB` — universal envelope every
-- case-execute path stamps with the LLM call's `CallProvenance`
-- (prompt_id + prompt_version + prompt_render_hash + model_id +
-- max_tokens + temperature). Eval failure → exact prompt + model
-- + render hash → drill-down without re-running. Same shape used
-- by `ArtifactProvenance` for ontology design and (future)
-- `graph_community_summaries.metadata` for GraphRAG indexing,
-- so the operator triaging a regression sees the same fields
-- regardless of which surface they clicked.

ALTER TABLE evaluation_cases
    ADD COLUMN metadata JSONB NOT NULL DEFAULT '{}'::jsonb;
