-- Phase 0 of the clean-refactor: make `source_id` a first-class column
-- on design_projects. Legacy design_projects always derived a SourceId
-- on-the-fly from SourceConfig fingerprint; persisting it lets federation
-- plan-cache keys, query provenance, and ambiguity lookups all refer to
-- the same stable string across requests and restarts.
--
-- The id follows the canonical `{source_type}:{source_fingerprint}`
-- rule encoded in `SourceId::from_source_config` — the backend
-- backfills any existing row on startup through the same rule, so the
-- column is `NOT NULL` immediately (no deferred constraint).

ALTER TABLE design_projects
    ADD COLUMN source_id TEXT NOT NULL DEFAULT '';

-- Backfill: derive `{source_type}:{source_fingerprint}` from existing
-- source_config JSON. Rows missing the fingerprint fall back to the
-- source_type alone; those ids are still stable for their duration,
-- and the analyzer will rewrite them once the project is next touched.
UPDATE design_projects
SET source_id = CASE
    WHEN source_config->>'source_fingerprint' IS NOT NULL
      AND source_config->>'source_fingerprint' <> ''
        THEN (source_config->>'source_type') || ':' || (source_config->>'source_fingerprint')
    ELSE (source_config->>'source_type')
END
WHERE source_id = '';

-- Drop the transient default so inserts are forced to supply a value
-- (the API layer builds it from the request-time SourceConfig). Any
-- insert that forgets to set the column fails loudly instead of
-- silently persisting an empty string.
ALTER TABLE design_projects
    ALTER COLUMN source_id DROP DEFAULT;

-- Indexed so federation plan-cache lookups by source id are O(log n).
CREATE INDEX design_projects_source_id_idx ON design_projects (source_id);
