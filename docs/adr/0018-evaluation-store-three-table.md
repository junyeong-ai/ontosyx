# 0018 — `EvaluationStore` three-table RAGAS-style metric loop

**Status:** Accepted

**Date:** 2026-05-05

**Supersedes:** none — `EvaluationStore` is a new persistence
surface introduced for the metric loop; no prior shape existed.

## Context

The platform's LLM-driven flows (NL→Cypher translation, GraphRAG
retrieval, agent tool use) need a first-class metric surface so
operators can:

- Run a curated dataset through the pipeline and score the output.
- Compare runs across model / prompt versions.
- Watch the metric trend regression-vs-baseline before promoting
  changes.

Industry-validated tools (RAGAS, DeepEval, Phoenix, Braintrust,
LangSmith) all converge on the same three-entity model:

- a **run** (one batch invocation),
- a **case** (one input/expected/actual triple inside the run),
- a **metric score** (one numeric axis on the case).

The case + metric split is load-bearing: a case captures the
shared prompt-response pair plus golden expectation, latency,
error path; metrics score it along independent axes
(faithfulness, relevance, precision, recall) that change
independently per judge prompt revision.

## Decision

Three workspace-scoped tables, all four-clause RLS:

- **`evaluation_runs`** — one row per evaluation batch.
  Carries `dataset_version_id` (FK to the dataset version this
  run scored), `prompt_render_hash` (the deterministic prompt
  fingerprint), `model_id`, lifecycle status, and timing.
- **`evaluation_cases`** — one row per `(run, input)` pair.
  UPSERT key `(run_id, case_key)` so re-running a dataset
  replaces in place. Carries `input` JSONB, `expected` JSONB,
  `actual` JSONB, `latency_ms`, `error_class`.
- **`evaluation_metrics`** — one row per `(case, axis)` score.
  UPSERT key `(case_id, name)` so re-judging the same axis
  replaces in place. Carries `score: f64`, `reasoning: String`,
  `metadata: JSONB`.

Open metric-name shape (`name: String` rather than a closed
enum) so adding a new RAGAS axis or product-specific score
(`schema_validity`, `pii_leak_count`) is a fresh `INSERT`,
never a DDL change. The closed enum lives in the *judge* layer
(`EvaluationJudgement::axes()` is the canonical iterator the
endpoint walks); the *storage* layer stays open so
operator-authored axes don't require a migration.

## Capture pipeline

`EvaluationContext` is a task-local on `ox-store`:

```rust
ox_store::scope_evaluation_context(ctx, async {
    /* anything inside this scope sees ctx via current_evaluation_context() */
}).await
```

`EvaluationCapture` is the trait the store implements; the
Brain's `call_structured_traced` helper checks for both
`current_evaluation_context()` AND `evaluation_capture.as_ref()`
before recording — production traffic without an evaluation
scope pays nothing. Capture-side write failures are logged at
`warn` and dropped; the LLM call already succeeded, the metric
is observability, not load-bearing.

The case + metric split is the dispatch boundary: case-execute
endpoints land `input` / `actual` / `latency` on the case;
judge endpoints (`POST /api/evaluation/cases/{id}/judge`) read
the case and write metrics rows. Re-judging UPSERTs in place
without disturbing the latency metric.

## Consequences

- **Adding a new axis is one INSERT-shape.** The judge prompt
  revision runs, and the metric row appears alongside the
  existing axes. No schema change, no UI change beyond the
  axis label.
- **Adding a new operation kind is the same axis.** A new
  `ExecuteEvaluationCaseRequest` variant + a Brain trait
  method + a dispatch arm + an FE option. No schema change,
  no new endpoint per kind.
- **Replay determinism is preserved.** The
  `prompt_render_hash` on the run row + the `dataset_version_id`
  on the run row + the `model_id` together pin "what prompt
  ran against what dataset under what model"; re-running with
  the same triple should produce the same metrics under
  deterministic decoding.
- **Online sampling slots in cleanly.** A future production-
  sampling middleware binds a synthetic `EvaluationContext`
  for 1% of `/api/chat` traffic; the capture trait writes the
  case + latency, an async judge worker fills the metric rows
  out-of-band. No schema change required.

## Open follow-ups

The three-table substrate is committed; the production-grade
surface still needs:

- **`evaluation_datasets` + `evaluation_dataset_versions`**
  tables (the `dataset_version_id` FK target). Today the
  dataset is implicit in the run's case set; landing the
  explicit entities makes regression / comparison runs
  reproducible.
- **CI golden gate** that runs the dataset on PR and fails
  when the mean faithfulness regresses below threshold.
- **Token + cost capture** on `EvaluationCapture` — the
  `latency_ms` axis lands today; cost / token use needs a
  matching `record_tokens` / `record_cost_usd` axis on the
  trait (with noop default to keep existing call-sites
  compiling).
- **Trace correlation** — persist `CallProvenance` (the
  per-call resolved model id + prompt fingerprint) on
  `evaluation_cases.metadata` so eval failures are
  end-to-end debuggable.
- **Multi-judge consensus / Cohen's kappa** for axis-quality
  meta-evaluation.
- **Async judge worker** + cron sweep over cases with
  `actual` and no judge metric.

These each have a memory note tracking the deferred state.

## Alternatives considered

- **Single-table "rich" runs** with embedded JSON for
  cases + metrics — rejected. Pivot queries (mean of axis
  X across all cases) become full table scans of JSON;
  re-running a single case becomes "rewrite the run row".
- **Closed metric enum at the storage layer** — rejected.
  Adding a workspace-specific axis would require a DDL
  migration, breaking the operator's ability to author
  domain-specific scores without a release.
- **Inline judge invocation on case-execute** — rejected.
  Judging is an LLM call; making case-execute a two-LLM
  call burdens the latency budget for cases that don't
  need an immediate score.

## References

- RAGAS — <https://github.com/explodinggradients/ragas>
- DeepEval — <https://github.com/confident-ai/deepeval>
- Phoenix Arize — <https://arize.com/phoenix>
- Braintrust — <https://braintrust.dev>
- Memory entry: `feedback_evaluation_store_pattern.md`
- Schema `crates/ox-store/migrations/0001_schema.sql`
- Trait: `crates/ox-store/src/store.rs` `EvaluationStore`
- Hook: `crates/ox-brain/src/lib.rs` `EvaluationCapture`
