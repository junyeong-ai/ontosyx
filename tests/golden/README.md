# Golden eval datasets

`tests/golden/*.golden.json` files are the platform's
operator-grade eval-driven regression baselines. Each file
declares a curated dataset the future
`scripts/eval-pr-gate.sh` (per Phase 8 of the long-horizon
work plan) feeds into the platform pipeline on every PR;
the gate refuses the merge when an axis score regresses
below its declared threshold.

## File contract

```jsonc
{
  "version": 1,                       // schema version of the golden file
  "description": "...",               // operator-readable scope
  "metadata": {
    "ontology_fixture": "tests/fixtures/...",
    "min_faithfulness": 0.85,         // RAGAS axis thresholds (0.0–1.0)
    "min_answer_relevance": 0.80,
    "min_context_precision": 0.75,
    "min_context_recall": 0.75,
    "judge_prompt": "evaluation_judge",
    "judge_prompt_version": "1.0.0"
  },
  "cases": [
    {
      "case_key": "single-node-by-label",   // stable id; UPSERT key
      "question": "...",                    // the operator NL prompt
      "expected_query_op_kind": "match",    // closed-form expected shape
      "expected_node_labels": ["Customer"],
      "axis_notes": {                       // per-axis grading hints
        "faithfulness": "...",
        "context_relevance": "..."
      }
    }
  ]
}
```

Field discipline:

- **`case_key`** is the UPSERT key against `evaluation_cases`
  (per ADR-0018). Re-running this dataset replaces the case
  in place; renaming a `case_key` orphans history.
- **`expected_*`** fields are closed-form constraints the judge
  prompt checks against the actual output. The judge does not
  pattern-match Cypher strings — it scores semantic alignment
  against the constraint. New constraint shapes require a
  matching arm in the judge prompt; pinning the
  `judge_prompt_version` makes the contract reproducible.
- **`axis_notes`** are operator-readable grading hints, not
  scoring inputs. They document why a particular case
  exercises a particular axis so the next reviewer can edit
  the case without losing the intent.

## Adding a case

Each case names exactly **one** retrievable concept the
platform should recognise; multi-axis cases drown a
regression's signal because the judge can't tell which
axis broke. Split a "show me last week's high-value orders
grouped by region" question into:
- `time-window-filter` (dates resolve correctly),
- `aggregate-by-group` (group-by is right),
- `value-threshold-filter` (threshold predicate is right).

## Future wiring

The CI gate (Phase 8) pulls this file at
`scripts/eval-pr-gate.sh` time, runs each case via
`POST /api/evaluation/runs/{run_id}/cases/{case_key}/execute`,
runs the judge endpoint per case, and refuses the PR when
the mean of any axis falls below `metadata.min_<axis>`.

Until the gate ships, this file is operator-readable
documentation of the platform's expected NL-input behaviour
and a shared baseline reviewers can grep against when
checking "is this question already covered".

## Related

- ADR-0018 — `EvaluationStore` three-table substrate
- `prompts/evaluation_judge.toml` — RAGAS four-axis judge
- Phase 8 of the long-horizon work plan (deferred from
  iteration to iteration because it needs both the dataset
  / experiment entities + the CI workflow + the FE
  dashboard surface)
