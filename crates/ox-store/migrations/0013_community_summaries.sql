-- 0013_community_summaries.sql
--
-- Microsoft GraphRAG-style community summary primitive. The
-- platform's GraphRAG retrieval path enriches LLM context with
-- entity-level anchors today (`ontology_entity_search_vector`
-- + `ontology_entity_neighbors`); community summaries layer
-- "what does this cluster of entities collectively represent?"
-- on top so a question like "how does the customer base look"
-- — broader than any single entity — reaches a relevant
-- summary rather than dissolving in single-entity matches.
--
-- The contract:
--
-- - **Hierarchical** — `level` field. Level 0 is the top-of-
--   tree (broadest summary covering many entities); higher
--   levels are narrower nested communities. Microsoft's
--   recursive Leiden produces 3-5 levels typically; the
--   platform doesn't pin the depth.
-- - **Identity-stable per version** — `(ontology_version_id,
--   community_id)` UNIQUE. Re-summarising under the same
--   community id replaces the prose; lineage stays attached.
-- - **Member-rich** — `member_entity_kinds` /
--   `member_logical_ids` parallel arrays carry the entity
--   composition so the FE renders cluster membership without
--   a follow-up join.
-- - **Searchable via gin_trgm** — title + summary indexed for
--   the retrieval path; the future
--   `OntologyCommunitySummaryStore::search_*` method walks
--   pg_trgm against an operator question to surface relevant
--   communities alongside the entity-level blend.
--
-- Detection / LLM summarisation are deferred — this migration
-- ships the storage primitive so operators can author summaries
-- manually first (e.g. via curl or a future admin form) and
-- the retrieval path can already use them. Auto-detection
-- (Leiden) + auto-summarisation (LLM) ride a future cron.

CREATE TABLE ontology_community_summaries (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id UUID NOT NULL DEFAULT (current_setting('app.workspace_id', true))::uuid
        REFERENCES workspaces(id) ON DELETE CASCADE,
    ontology_version_id UUID NOT NULL
        REFERENCES ontology_version_snapshots(id) ON DELETE CASCADE,
    -- Community identifier — workspace-supplied at author-time
    -- (e.g. "customer_segment_premium") or detection-generated
    -- (e.g. "leiden:level-1:cluster-7"). Stable across
    -- re-summarisation under the same community_id.
    community_id TEXT NOT NULL,
    level SMALLINT NOT NULL DEFAULT 0,
    -- Parallel arrays: kind[i] + logical_id[i] together
    -- identify one member entity. TEXT[] over JSONB because
    -- gin_array indexing on individual logical_ids is the
    -- canonical path for "which communities mention this
    -- entity?" lookups.
    member_entity_kinds TEXT[] NOT NULL DEFAULT '{}',
    member_logical_ids TEXT[] NOT NULL DEFAULT '{}',
    title TEXT NOT NULL,
    summary TEXT NOT NULL,
    generated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (ontology_version_id, community_id)
);

CREATE INDEX ontology_community_summaries_version_idx
    ON ontology_community_summaries (ontology_version_id);
CREATE INDEX ontology_community_summaries_summary_trgm_idx
    ON ontology_community_summaries USING gin (summary gin_trgm_ops);
CREATE INDEX ontology_community_summaries_title_trgm_idx
    ON ontology_community_summaries USING gin (title gin_trgm_ops);
-- Reverse index: "which communities contain entity X?" for
-- the retrieval path's anchor-to-community walk.
CREATE INDEX ontology_community_summaries_logical_ids_idx
    ON ontology_community_summaries USING gin (member_logical_ids);

-- 4-clause RLS — required by the
-- `workspace_scoped_tables_have_full_rls_protection` invariant
-- test (every workspace_id table must have ENABLE + FORCE +
-- tenant-gate + system_bypass).
ALTER TABLE ontology_community_summaries ENABLE ROW LEVEL SECURITY;
ALTER TABLE ontology_community_summaries FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON ontology_community_summaries
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON ontology_community_summaries
    USING (current_setting('app.system_bypass', true) = 'true');
