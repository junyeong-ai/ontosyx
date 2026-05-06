-- 0011_evaluation_datasets.sql
--
-- First-class `EvaluationDataset` entity — frozen Q+expected pairs
-- reusable across runs. Phoenix / Braintrust / LangSmith industry
-- pattern: a dataset is the unit of input authoring, a run is the
-- unit of execution against a dataset + model + config snapshot.
-- Pairing the two unlocks (a) regression diff between runs over the
-- same dataset, (b) CI golden gate (sealed dataset, threshold on
-- post-judge metrics), (c) cross-team baseline reuse.
--
-- Two tables:
--   - `evaluation_datasets`        — header (name, description)
--   - `evaluation_dataset_items`   — frozen rows (`item_key` + `input` + `expected`)
--
-- `evaluation_runs` gains an optional `dataset_id` FK so a run that
-- materialised from a dataset records the lineage. NULL for ad-hoc
-- runs whose cases were inserted directly via the bulk-upsert path.
--
-- Cascade semantics:
--   - Deleting a dataset cascades to its items but NOT the runs that
--     used it (runs go SET NULL on the FK so historical run rows
--     stay readable).

CREATE TABLE evaluation_datasets (
    id          UUID PRIMARY KEY,
    workspace_id UUID NOT NULL,
    name        TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (workspace_id, name)
);

CREATE INDEX evaluation_datasets_workspace_id
    ON evaluation_datasets (workspace_id);

ALTER TABLE evaluation_datasets ENABLE ROW LEVEL SECURITY;
ALTER TABLE evaluation_datasets FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON evaluation_datasets
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON evaluation_datasets
    USING (current_setting('app.system_bypass', true) = 'true');

CREATE TABLE evaluation_dataset_items (
    id          UUID PRIMARY KEY,
    dataset_id  UUID NOT NULL REFERENCES evaluation_datasets(id) ON DELETE CASCADE,
    workspace_id UUID NOT NULL,
    -- Stable per-dataset identifier (e.g. "q01" / "golden_001").
    -- UNIQUE with `dataset_id` so re-importing a dataset from CSV /
    -- JSON replaces previous rows via UPSERT on the natural key.
    item_key    TEXT NOT NULL,
    -- Prompt / context envelope. JSONB so the input shape grows
    -- with the evaluator (single question today, multi-turn
    -- conversation tomorrow) without a schema change. Mirrors
    -- `evaluation_cases.input` so `create_run_from_dataset` is a
    -- straight copy without transform.
    input       JSONB NOT NULL,
    -- Golden / reference outcome. NULL when the dataset is
    -- unsupervised (production replay).
    expected    JSONB,
    -- Free-form authoring metadata (tags, difficulty, locale, …).
    -- Surfaces verbatim on the FE detail panel; doesn't propagate
    -- to per-run case rows by default — runs that need item
    -- metadata copy via `create_run_from_dataset`'s metadata-pin
    -- knob.
    metadata    JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (dataset_id, item_key)
);

CREATE INDEX evaluation_dataset_items_dataset_id
    ON evaluation_dataset_items (dataset_id);

ALTER TABLE evaluation_dataset_items ENABLE ROW LEVEL SECURITY;
ALTER TABLE evaluation_dataset_items FORCE ROW LEVEL SECURITY;
CREATE POLICY ws_isolation ON evaluation_dataset_items
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON evaluation_dataset_items
    USING (current_setting('app.system_bypass', true) = 'true');

-- Run ↔ dataset lineage. NULL preserves historical ad-hoc runs.
ALTER TABLE evaluation_runs
    ADD COLUMN dataset_id UUID REFERENCES evaluation_datasets(id) ON DELETE SET NULL;

CREATE INDEX evaluation_runs_dataset_id
    ON evaluation_runs (dataset_id);
