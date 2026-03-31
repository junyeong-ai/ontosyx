-- 0002: Add missing indices for query performance
--
-- These indices address frequently filtered columns that lacked proper indexing,
-- identified during the 2026-03 codebase audit.

-- design_projects: filtered by status in list operations
CREATE INDEX IF NOT EXISTS idx_design_projects_status
    ON design_projects(status)
    WHERE archived_at IS NULL;

-- agent_sessions: retention cleanup queries filter by completed_at
CREATE INDEX IF NOT EXISTS idx_agent_sessions_completed
    ON agent_sessions(completed_at)
    WHERE completed_at IS NOT NULL;

-- analysis_results: recipe result lookup ordered by recency
CREATE INDEX IF NOT EXISTS idx_analysis_results_recipe_created
    ON analysis_results(recipe_id, created_at DESC);

-- memory_entries: metadata ontology_id filtering for scoped recall
CREATE INDEX IF NOT EXISTS idx_memory_metadata_ontology
    ON memory_entries ((metadata->>'ontology_id'));
