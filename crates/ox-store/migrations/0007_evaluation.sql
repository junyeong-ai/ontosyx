-- 0007_evaluation.sql
--
-- Evaluation surface — the platform's first-class metric loop for
-- LLM-driven flows (NL→Cypher translation, GraphRAG retrieval,
-- agent tool use). Three tables, all workspace-scoped, all under
-- the canonical 4-clause RLS pattern.
--
-- Schema sketch:
--
--   evaluation_runs           — one row per evaluation batch.
--   evaluation_cases          — one row per (run, input) pair.
--   evaluation_metrics        — one row per (case, metric_name).
--
-- The case + metric split mirrors RAGAS / DeepEval: a case captures
-- the prompt-response pair plus its golden expectation and
-- timing, and 0..N metrics score that case along independent
-- axes (faithfulness, answer_relevance, context_precision,
-- context_recall, latency_p95, …). Adding a new axis is a fresh
-- metric row, never a new column.
--
-- `ontology_version_id` is intentionally nullable — greenfield
-- evaluations run against an ontology draft that has not yet
-- been committed. Once a canonical version exists the column
-- pins the run to it, so historical metrics stay associated
-- with the schema they were measured against.

CREATE TABLE evaluation_runs (
    id UUID PRIMARY KEY,
    workspace_id UUID NOT NULL,
    -- Optional pin to a committed ontology version. NULL allowed
    -- so a draft-stage evaluation can record metrics before the
    -- workspace's first canonical version exists.
    ontology_version_id UUID,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    -- Wire enum: 'running' | 'succeeded' | 'failed' | 'cancelled'.
    -- Kept as TEXT so adding a new variant is a Rust-side change
    -- with no migration; the parity test pins the string set.
    status TEXT NOT NULL,
    started_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at TIMESTAMPTZ,
    -- JSONB envelope for run-level configuration (model, dataset
    -- ref, judge model, …). Schema-less by design — adding a new
    -- run-level dimension never needs DDL.
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb
);

CREATE INDEX evaluation_runs_workspace_recent
    ON evaluation_runs (workspace_id, started_at DESC);

ALTER TABLE evaluation_runs ENABLE ROW LEVEL SECURITY;
ALTER TABLE evaluation_runs FORCE ROW LEVEL SECURITY;

CREATE POLICY ws_isolation ON evaluation_runs
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);

CREATE POLICY system_bypass ON evaluation_runs
    USING (current_setting('app.system_bypass', true) = 'true')
    WITH CHECK (current_setting('app.system_bypass', true) = 'true');


CREATE TABLE evaluation_cases (
    id UUID PRIMARY KEY,
    run_id UUID NOT NULL REFERENCES evaluation_runs(id) ON DELETE CASCADE,
    workspace_id UUID NOT NULL,
    -- Stable per-run identifier (e.g. "q01" / "golden_001"). UNIQUE
    -- with `run_id` so re-running a dataset replaces previous rows
    -- via UPSERT on the natural key.
    case_key TEXT NOT NULL,
    -- Prompt / context envelope. JSONB so the case input shape
    -- can grow with the evaluator (single question today, multi-
    -- turn conversation tomorrow) without a schema change.
    input JSONB NOT NULL,
    -- Golden / reference outcome. NULL when the dataset is
    -- unsupervised (production replay, exploratory evaluation).
    expected JSONB,
    -- Observed outcome. NULL until the evaluator records the run;
    -- a NULL `actual` with a NON-NULL `error` indicates the case
    -- threw before producing output.
    actual JSONB,
    error TEXT,
    latency_ms BIGINT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (run_id, case_key)
);

CREATE INDEX evaluation_cases_workspace_run
    ON evaluation_cases (workspace_id, run_id);

ALTER TABLE evaluation_cases ENABLE ROW LEVEL SECURITY;
ALTER TABLE evaluation_cases FORCE ROW LEVEL SECURITY;

CREATE POLICY ws_isolation ON evaluation_cases
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);

CREATE POLICY system_bypass ON evaluation_cases
    USING (current_setting('app.system_bypass', true) = 'true')
    WITH CHECK (current_setting('app.system_bypass', true) = 'true');


CREATE TABLE evaluation_metrics (
    id UUID PRIMARY KEY,
    case_id UUID NOT NULL REFERENCES evaluation_cases(id) ON DELETE CASCADE,
    workspace_id UUID NOT NULL,
    -- Metric name — a free-form string by intent. RAGAS axes
    -- ('faithfulness', 'answer_relevance', 'context_precision',
    -- 'context_recall') are the seed set; tenant-defined metrics
    -- ride on the same column without DDL.
    name TEXT NOT NULL,
    -- Score normalised to [0.0, 1.0]. DOUBLE PRECISION rather
    -- than NUMERIC so sqlx maps it to f64 directly (NUMERIC →
    -- Decimal forces a manual conversion at every boundary).
    score DOUBLE PRECISION NOT NULL,
    -- Optional natural-language reasoning emitted by the LLM
    -- judge or operator. Absent for code-side rubrics that don't
    -- carry prose.
    reasoning TEXT,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- One score per (case, metric_name). Re-running the judge
    -- replaces via UPSERT on the natural key — the latest score
    -- wins, history goes through `evaluation_metric_revisions`
    -- (deferred; not needed for the MVP).
    UNIQUE (case_id, name)
);

CREATE INDEX evaluation_metrics_workspace_case
    ON evaluation_metrics (workspace_id, case_id);

ALTER TABLE evaluation_metrics ENABLE ROW LEVEL SECURITY;
ALTER TABLE evaluation_metrics FORCE ROW LEVEL SECURITY;

CREATE POLICY ws_isolation ON evaluation_metrics
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);

CREATE POLICY system_bypass ON evaluation_metrics
    USING (current_setting('app.system_bypass', true) = 'true')
    WITH CHECK (current_setting('app.system_bypass', true) = 'true');
