// Wire shape for `ox_store::evaluation::*`. Server owns `id`,
// `started_at`, `completed_at`, `created_at` — clients read them,
// never set. The status enum is the snake_case storage tag.

export type EvaluationRunStatus =
  | "running"
  | "succeeded"
  | "failed"
  | "cancelled";

export interface EvaluationRun {
  id: string;
  workspace_id: string;
  /** Optional pin to a committed ontology version. Absent for
   *  pre-canonical / draft-stage evaluations. */
  ontology_version_id?: string;
  name: string;
  description?: string;
  status: EvaluationRunStatus;
  started_at: string;
  completed_at?: string;
  /** Schema-less run-level config envelope. */
  metadata: Record<string, unknown>;
}

export interface EvaluationCase {
  id: string;
  run_id: string;
  workspace_id: string;
  case_key: string;
  input: unknown;
  expected?: unknown;
  actual?: unknown;
  error?: string;
  latency_ms?: number;
  created_at: string;
}

export interface EvaluationMetric {
  id: string;
  case_id: string;
  workspace_id: string;
  /** RAGAS canonicals (`faithfulness`, `answer_relevance`,
   *  `context_precision`, `context_recall`) plus tenant-defined
   *  axes — same column. */
  name: string;
  /** Conventionally `[0.0, 1.0]`; the column is unbounded so a
   *  rubric with a different domain (latency p95 ms) can ride on
   *  the same shape. */
  score: number;
  reasoning?: string;
  metadata: Record<string, unknown>;
  created_at: string;
}

export interface CreateEvaluationRunRequest {
  name: string;
  description?: string;
  ontology_version_id?: string;
  metadata?: Record<string, unknown>;
}

export interface BulkUpsertEvaluationCaseEntry {
  case_key: string;
  input: unknown;
  expected?: unknown;
}

export interface BulkUpsertEvaluationCasesRequest {
  cases: BulkUpsertEvaluationCaseEntry[];
}

export interface BulkUpsertEvaluationCaseError {
  case_key: string;
  message: string;
}

export interface BulkUpsertEvaluationCasesResponse {
  upserted_count: number;
  /** Empty when every row landed; non-empty when partial-success.
   *  The caller retries just the listed `case_key`s. */
  errors: BulkUpsertEvaluationCaseError[];
}

export interface UpsertEvaluationCaseRequest {
  case_key: string;
  input: unknown;
  expected?: unknown;
  actual?: unknown;
  error?: string;
  latency_ms?: number;
}

export interface RecordEvaluationMetricRequest {
  name: string;
  score: number;
  reasoning?: string;
  metadata?: Record<string, unknown>;
}

/**
 * Discriminated request envelope for `POST /api/evaluation/runs/{run_id}/cases/{case_key}/execute`.
 * Adding a new operation lands as a fresh variant with the
 * matching backend dispatch arm — the wrapping endpoint /
 * scope / latency-capture flow stays shared.
 */
export type ExecuteEvaluationCaseRequest =
  | {
      kind: "translate_query";
      question: string;
      /** Optional golden `QueryIR` for downstream judge comparison. */
      expected_query_ir?: unknown;
    }
  | {
      kind: "explain";
      question: string;
      /** Optional reference answer for downstream comparison. */
      expected_answer?: string;
    }
  | {
      kind: "retrieve_anchors";
      question: string;
      /** Top-K cap on the retrieval set. The server clamps
       *  to `[1, 100]` to match the navigation store's
       *  `EntryPointSearchOptions.limit` ceiling. */
      top_k: number;
      /** Gold-standard anchor logical ids in `kind:logical_id`
       *  form (`node_type:Customer`, `glossary_term:gt-vip`).
       *  Stored on `evaluation_cases.expected` so the dataset
       *  survives re-runs. Empty list is allowed — the case
       *  scores precision at 0 with vacuous recall 1. */
      expected_anchor_ids: string[];
    };

/** Operation kinds the case-execute endpoint dispatches on. The
 *  closed union mirrors the BE `ExecuteEvaluationCaseRequest`
 *  enum — adding a new kind lands as a new wire variant + UI
 *  choice in lockstep. */
export const EXECUTE_OPERATION_KINDS = [
  "translate_query",
  "explain",
  "retrieve_anchors",
] as const;
export type ExecuteOperationKind = (typeof EXECUTE_OPERATION_KINDS)[number];

export interface CompleteEvaluationRunRequest {
  /** Terminal state — must be `succeeded` / `failed` /
   *  `cancelled`. The server rejects `running` with a typed 422. */
  status: Exclude<EvaluationRunStatus, "running">;
}

export interface EvaluationRunListPage {
  items: EvaluationRun[];
  next_cursor?: string;
}

/** One per-case axis-level diff between two runs over the same
 *  dataset. Mirrors `ox_store::evaluation::RunMetricDelta`. */
export interface RunMetricDelta {
  case_key: string;
  axis: string;
  baseline_score: number;
  candidate_score: number;
  /** `candidate_score - baseline_score`. Positive = candidate
   *  improved; negative = regression. */
  delta: number;
}

/** Per-axis aggregate roll-up across every (case_key, axis) pair
 *  both runs share. Mirrors `ox_store::evaluation::RunAxisSummary`. */
export interface RunAxisSummary {
  axis: string;
  paired_case_count: number;
  baseline_mean: number;
  candidate_mean: number;
  /** `candidate_mean - baseline_mean`. */
  mean_delta: number;
  /** Percentage of paired cases where candidate beats baseline.
   *  `[0.0, 100.0]`. Ties count as half a win. */
  win_rate_pct: number;
  /** Cohen's d effect size — `(mean_c - mean_b) / pooled_std`.
   *  Industry interpretation: `|d| < 0.2` negligible, `0.5`
   *  medium, `0.8` large. `undefined` when both runs produced
   *  identical scores (zero pooled variance). */
  cohen_d?: number;
}

/** Two-run comparison report shape. Mirrors
 *  `ox_store::evaluation::RunComparisonReport`. */
export interface RunComparisonReport {
  baseline_run_id: string;
  candidate_run_id: string;
  /** Pinned dataset both runs reference. The BE rejects diff
   *  between runs over different datasets with a typed 400. */
  dataset_id: string;
  per_case: RunMetricDelta[];
  per_axis: RunAxisSummary[];
}

/** Per-axis aggregate for a single run. Mirrors
 *  `ox_store::evaluation::AxisAggregate`. */
export interface AxisAggregate {
  axis: string;
  mean: number;
  count: number;
}

/** Run-level summary returned by the
 *  `/api/evaluation/runs/{run_id}/summary` endpoint. */
export interface RunSummary {
  run_id: string;
  total_cases: number;
  /** Cases with at least one RAGAS-tagged metric. The badge
   *  reads as "judged X of Y". */
  judged_cases: number;
  /** Cases with `error IS NOT NULL` — case-execute failed. */
  failed_cases: number;
  /** Per-axis (mean, count). Sorted alphabetically by axis. */
  axis_means: AxisAggregate[];
}

/** Mirrors `ox_store::evaluation::EvaluationDataset` —
 *  workspace-scoped, name-keyed via UPSERT so re-import
 *  preserves `id` + `created_at`. */
export interface EvaluationDataset {
  id: string;
  workspace_id: string;
  name: string;
  description: string;
  created_at: string;
}

export interface EvaluationDatasetListPage {
  items: EvaluationDataset[];
  next_cursor?: string;
}

export interface UpsertEvaluationDatasetRequest {
  name: string;
  description?: string;
}

/** Mirrors `ox_store::evaluation::EvaluationDatasetItem`. */
export interface EvaluationDatasetItem {
  id: string;
  dataset_id: string;
  workspace_id: string;
  item_key: string;
  input: unknown;
  expected?: unknown;
  metadata: Record<string, unknown>;
  created_at: string;
}
