# Evaluation production-grade — Dataset + Experiment + CI gate

**Status:** Design sketch — Phase 8 of the long-horizon
work plan. The substrate is committed (ADR-0018's
three-table `EvaluationStore`); the production-grade
surface — versioned datasets, experiment comparison,
trace correlation, CI gate — needs concentrated
multi-iteration work. Landing the design sketch as
`docs/architecture/evaluation-production-design.md`
captures the contract so the next session has the
schema, the lifecycle, and the integration points in
one place.

## What's already shipped (per ADR-0018)

- Three workspace-scoped tables: `evaluation_runs`,
  `evaluation_cases`, `evaluation_metrics`.
- `EvaluationContext` task-local + `EvaluationCapture`
  trait; the Brain's `call_structured_traced`
  records `latency_ms.<operation>` whenever an
  evaluation scope is bound.
- RAGAS four-axis judge (`faithfulness`,
  `answer_relevance`, `context_precision`,
  `context_recall`).
- `tests/golden/nl2cypher.golden.json` — the first
  curated dataset (per the iteration-13 commit).

## What this design adds

Five deliverables, each independently shippable:

### 1. Versioned datasets

New tables:

```sql
CREATE TABLE evaluation_datasets (
    id            uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id  uuid NOT NULL DEFAULT (current_setting('app.workspace_id', true))::uuid,
    name          text NOT NULL,
    description   text,
    created_at    timestamptz NOT NULL DEFAULT now(),
    UNIQUE (workspace_id, name)
);

CREATE TABLE evaluation_dataset_versions (
    id            uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id  uuid NOT NULL DEFAULT (current_setting('app.workspace_id', true))::uuid,
    dataset_id    uuid NOT NULL REFERENCES evaluation_datasets(id) ON DELETE CASCADE,
    version       integer NOT NULL,            -- monotonic per dataset
    cases         jsonb NOT NULL,               -- frozen case manifest at version time
    committed_by  text NOT NULL,
    commit_message text,
    created_at    timestamptz NOT NULL DEFAULT now(),
    UNIQUE (dataset_id, version)
);
-- + four-clause RLS per the canonical pattern
```

`evaluation_runs.dataset_version_id` becomes a NOT NULL
FK; every run is reproducible against the exact case
set it scored.

The `tests/golden/nl2cypher.golden.json` fixture
loads on first boot as the seed `nl2cypher` dataset
version 1; subsequent edits to the file commit version
2, 3, … with the operator's `commit_message` captured.

### 2. Experiment + comparison surface

A new `evaluation_experiments` table groups N runs
that share a dataset version, vary on one axis
(prompt-template-version, model, retrieval mode):

```sql
CREATE TABLE evaluation_experiments (
    id            uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id  uuid NOT NULL DEFAULT (current_setting('app.workspace_id', true))::uuid,
    name          text NOT NULL,
    description   text,
    dataset_version_id uuid NOT NULL REFERENCES evaluation_dataset_versions(id),
    baseline_run_id uuid REFERENCES evaluation_runs(id),
    created_at    timestamptz NOT NULL DEFAULT now(),
    UNIQUE (workspace_id, name)
);
```

`evaluation_runs.experiment_id` becomes an optional FK;
ad-hoc runs without an experiment stay supported. The
FE comparison surface picks an experiment, picks two
runs, and renders per-case axis deltas + win-rate +
Cohen's d.

The comparison endpoint:

```
GET /api/evaluation/experiments/{exp_id}/compare?baseline={run_id}&candidate={run_id}
```

Returns:

```jsonc
{
  "axis_summary": {
    "faithfulness": { "baseline_mean": 0.87, "candidate_mean": 0.91, "delta": 0.04, "win_rate": 0.62 },
    "answer_relevance": { ... },
    "context_precision": { ... },
    "context_recall": { ... }
  },
  "per_case_diff": [
    { "case_key": "single-node-by-label", "axis_deltas": { "faithfulness": 0.12, ... } },
    ...
  ]
}
```

### 3. Trace correlation

Persist `CallProvenance` (the per-call resolved
`(model_id, prompt_template_id, prompt_template_version,
prompt_render_hash, token_usage)` envelope) on
`evaluation_cases.metadata`. Adds two columns:

- `request_id text` — threads through from the
  `x-request-id` middleware header so the eval row
  joins to the audit trail.
- `call_provenance jsonb` — typed
  `Vec<CallProvenance>` (one entry per LLM call inside
  the case execution; Brain → judge → repair calls
  each contribute).

The FE drilldown UI then renders "case `X` failed
faithfulness; click through to see the prompt that
ran, the model, the tokens consumed". Without this,
eval failures are not debuggable end-to-end.

### 4. Token + cost capture

Two new methods on `EvaluationCapture` (with default
noop implementations to keep existing consumers
compiling):

```rust
async fn record_tokens(
    &self,
    ctx: &EvaluationContext,
    operation: &str,
    prompt_tokens: u64,
    completion_tokens: u64,
);

async fn record_cost_usd(
    &self,
    ctx: &EvaluationContext,
    operation: &str,
    cost_usd: f64,
);
```

Both land as `evaluation_metrics` rows
(`name = "tokens.prompt"`, `name = "tokens.completion"`,
`name = "cost_usd"`) so the existing axis-aggregation
path picks them up without a schema change.

### 5. CI gate

`scripts/eval-pr-gate.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

# 1. Spin up a fresh test DB (sqlx migrate)
# 2. Boot ox-api against it
# 3. Seed the workspace + the canonical_ecommerce ontology fixture
# 4. Create an evaluation run against the latest nl2cypher.golden version
# 5. Iterate cases: POST /api/evaluation/runs/{id}/cases/{key}/execute
#    POST /api/evaluation/cases/{case_id}/judge
# 6. Aggregate: SELECT AVG(score) FROM evaluation_metrics WHERE run_id = $1 GROUP BY name
# 7. Compare against tests/golden/nl2cypher.golden.json metadata.min_<axis>
# 8. Exit non-zero if any axis regresses below threshold
```

The `.github/workflows/eval.yml` runs the script on
every PR that touches `crates/ox-brain/`,
`prompts/`, `crates/ox-graph-runtime/dialect/cypher/`,
or `tests/golden/`. The threshold floor lives in the
golden file's `metadata.min_<axis>` so lifting it is
a deliberate commit (operator decides the floor; the
script enforces).

## FE production-grade dashboard

The current `/settings/evaluation` page is minimal. The
production-grade dashboard adds:

- **Aggregations** per axis: mean / p50 / p95 / count.
- **Per-axis histograms** so a regression's distribution
  shape (drop in long-tail vs. mean) is visible at a
  glance.
- **Time-series** of axis means across the dataset's
  run history.
- **Run comparison** picker — pick baseline + candidate,
  see the diff per case + per axis.
- **Experiment view** — pick an experiment, see every
  run's axis means in a stacked-bar comparison.
- **Per-case JSON diff viewer** — input / expected /
  actual side-by-side with a structural diff highlight.
- **Trace drilldown** — click a failed case, see the
  `call_provenance` chain with prompt fingerprints +
  token counts.

## Out of scope (v1)

- **Async judge worker** — judging is sync today;
  `record_judge_async` cron would let the case-execute
  endpoint return faster while the judge runs in the
  background. v1 ships the sync path; the async worker
  is a Phase 9-class follow-up.
- **Online sampling middleware** — production traffic
  doesn't carry an `EvaluationContext` today
  (intentional per ADR-0018). A 1% sampler that binds
  a synthetic `EvaluationContext` for live traffic is
  a future deferred decision; the dataset / experiment
  scaffolding makes it straightforward to land when
  product-grade chat traffic exists.
- **Multi-judge consensus / Cohen's kappa** — single
  LLM judge has documented bias. v1 still ships
  single-judge; the multi-judge consensus is a Phase
  9-class follow-up that adds judge-vs-judge agreement
  scoring.
- **Human annotation queue** — labeling UI for
  reviewer-assigned `expected` values. v1 ships
  golden-fixture-only; the human queue lands when the
  operator surface needs it.
- **Synthetic test-set generation** — the RAGAS
  `TestsetGenerator` shape that produces N
  question/expected pairs from the ontology + sample
  data. v1 ships hand-curated golden fixtures; the
  generator lands when the dataset surface scales past
  what hand-curation handles.

## Test pyramid

- **Unit tests** on dataset versioning + experiment
  comparison math (Cohen's d on synthetic distributions).
- **Integration tests in `ox-api/tests/`** — full eval
  run end-to-end against the seeded
  `nl2cypher.golden.json` dataset; assert the
  comparison endpoint returns the expected delta
  shape.
- **CI gate dry-run** in PR — the
  `scripts/eval-pr-gate.sh` runs against a hosted
  PR-specific test DB. The gate's own tests live in
  `crates/ox-store/tests/evaluation_gate_*.rs`.

## References

- ADR-0017 — Typed error wire shape
  (`EvaluationDatasetVersionMismatch` etc.).
- ADR-0018 — `EvaluationStore` three-table substrate
  (already shipped).
- ADR-0023 — `HeuristicProposal` queue (the
  human-annotation queue, when it lands, rides this).
- `tests/golden/nl2cypher.golden.json` — the seed
  dataset.
- `prompts/evaluation_judge.toml` — the judge prompt.
- Phase 8 of the long-horizon plan.
- RAGAS — `https://github.com/explodinggradients/ragas`
- Phoenix Arize, Braintrust, LangSmith (industry
  comparison surfaces the FE design draws from).
