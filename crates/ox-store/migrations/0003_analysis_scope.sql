-- Promote per-project analysis scope to a first-class column.
--
-- The previous shape — `initial_selection: jsonb` capturing the
-- `AnalyzeSelection` chosen at project creation — kept the operator's
-- bootstrap intent but threw it away on every extend / reanalyze pass
-- (the four call sites in ox-api wrote `None` after the first
-- creation). The new shape — `analysis_scope: jsonb` — accumulates
-- across the project's lifetime: every selection that runs against
-- the project records its tables in the scope, deferred tables stay
-- explicit with reason + revisit timestamp, schema fingerprints land
-- alongside so drift detection can compare against the last
-- introspection. The FE renders it as the project header's
-- `n / N modeled · k deferred` badge and as the source-Inspector's
-- "Deferred" tab.
--
-- See `ox_source::AnalysisScope` for the wire shape.

ALTER TABLE design_projects DROP COLUMN initial_selection;
ALTER TABLE design_projects ADD COLUMN analysis_scope jsonb NOT NULL DEFAULT '{}'::jsonb;
