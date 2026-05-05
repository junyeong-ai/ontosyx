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

export interface CompleteEvaluationRunRequest {
  /** Terminal state — must be `succeeded` / `failed` /
   *  `cancelled`. The server rejects `running` with a typed 422. */
  status: Exclude<EvaluationRunStatus, "running">;
}

export interface EvaluationRunListPage {
  items: EvaluationRun[];
  next_cursor?: string;
}
