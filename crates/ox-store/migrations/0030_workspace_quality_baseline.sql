-- Workspace-level quality baseline for adaptive thresholds.
--
-- The QualityBanner currently reads hardcoded `DEFAULT_THRESHOLDS`
-- in the frontend (shacl_pass_rate: 0.9 warn / 0.8 critical, etc.).
-- Those are reasonable day-zero defaults but don't reflect what any
-- individual workspace's query stream actually looks like. After a
-- few weeks of observation, `median ± k·MAD` of each metric gives
-- a workspace-specific baseline the banner can switch to so alerts
-- fire on actual drift, not on global-prior deviation.
--
-- Phase A (this migration): snapshot the baseline nightly so the
-- data accumulates from day one. Banner wiring to these rows is
-- Phase B — deferred until a few weeks of observation validate the
-- MAD computation against real workloads.
--
-- Keyed by workspace only: a single workspace has one active
-- baseline; the cron upserts in place rather than appending a
-- history row, so consumers always read the latest state without
-- window-picking logic.

CREATE TABLE workspace_quality_baseline (
    workspace_id uuid PRIMARY KEY,

    -- Metric window the cron used to compute this snapshot ("7d" /
    -- "30d" / "90d"). Stored as text so new windows don't require a
    -- migration.
    window text NOT NULL DEFAULT '30d',

    -- Sample size that fed the computation. Frontend treats
    -- baselines with fewer than `MIN_SAMPLE_SIZE` signals as
    -- insufficient evidence and falls back to the hardcoded prior.
    sample_size bigint NOT NULL DEFAULT 0,

    -- Per-metric `{median, mad, warn, critical}` bundle. The cron
    -- computes `warn = median ± 2·MAD` and `critical = median ±
    -- 3·MAD` per metric key (shacl_pass_rate, query_reproducibility,
    -- anchor_match_rate, glossary_hit_rate, clarification_success_rate,
    -- stale_concept_ratio). JSONB rather than typed columns so new
    -- metrics land without a schema change — the banner's alert
    -- engine reads by key regardless.
    thresholds jsonb NOT NULL DEFAULT '{}'::jsonb,

    computed_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE workspace_quality_baseline ENABLE ROW LEVEL SECURITY;
ALTER TABLE workspace_quality_baseline FORCE ROW LEVEL SECURITY;

CREATE POLICY ws_isolation ON workspace_quality_baseline
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);

CREATE POLICY system_bypass ON workspace_quality_baseline
    USING (current_setting('app.system_bypass', true) = 'true')
    WITH CHECK (current_setting('app.system_bypass', true) = 'true');
