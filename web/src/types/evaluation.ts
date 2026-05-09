// Wire shape for `ox_store::evaluation::*`. Server owns `id`,
// `started_at`, `completed_at`, `created_at` — clients read them,
// never set. The status enum is the snake_case storage tag.

import type { components } from "./api.generated";
import type { ClientPage } from "./pagination";

export type EvaluationRunStatus = components["schemas"]["EvaluationRunStatus"];
export type EvaluationCaseInput = components["schemas"]["EvaluationCaseInput"];
export type EvaluationExpected = components["schemas"]["EvaluationExpected"];
export type EvaluationActual = components["schemas"]["EvaluationActual"];
export type EvaluationCaseMetadata =
  components["schemas"]["EvaluationCaseMetadata"];
export type EvaluationMetricMetadata =
  components["schemas"]["EvaluationMetricMetadata"];

export type EvaluationRun = components["schemas"]["EvaluationRun"];
export type EvaluationCase = components["schemas"]["EvaluationCase"];
export type EvaluationMetric = components["schemas"]["EvaluationMetric"];
export type CreateEvaluationRunRequest = components["schemas"]["CreateEvaluationRunRequest"];
export type BulkUpsertEvaluationCaseEntry = components["schemas"]["BulkUpsertEvaluationCaseEntry"];
export type BulkUpsertEvaluationCasesRequest = components["schemas"]["BulkUpsertEvaluationCasesRequest"];
export type BulkUpsertEvaluationCaseError = components["schemas"]["BulkUpsertEvaluationCaseError"];
export type BulkUpsertEvaluationCasesResponse = components["schemas"]["BulkUpsertEvaluationCasesResponse"];
export type UpsertEvaluationCaseRequest = components["schemas"]["UpsertEvaluationCaseRequest"];
export type RecordEvaluationMetricRequest = components["schemas"]["RecordEvaluationMetricRequest"];

/** Operation kinds the case-execute endpoint dispatches on. The
 *  closed union mirrors the BE `EvaluationCaseInput`
 *  enum — adding a new kind lands as a new wire variant + UI
 *  choice in lockstep. */
export const EXECUTE_OPERATION_KINDS = [
  "translate_query",
  "explain",
  "retrieve_anchors",
] as const;
export type ExecuteOperationKind = (typeof EXECUTE_OPERATION_KINDS)[number];

export type CompleteEvaluationRunRequest = components["schemas"]["CompleteEvaluationRunRequest"];
export type EvaluationRunListPage = ClientPage<components["schemas"]["EvaluationRunPage"]>;

/** One per-case axis-level diff between two runs over the same
 *  dataset. Mirrors `ox_store::evaluation::RunMetricDelta`. */
export type RunMetricDelta = components["schemas"]["RunMetricDelta"];

/** Per-axis aggregate roll-up across every (case_key, axis) pair
 *  both runs share. Mirrors `ox_store::evaluation::RunAxisSummary`. */
export type RunAxisSummary = components["schemas"]["RunAxisSummary"];

/** Two-run comparison report shape. Mirrors
 *  `ox_store::evaluation::RunComparisonReport`. */
export type RunComparisonReport = components["schemas"]["RunComparisonReport"];

/** Per-axis aggregate for a single run. Mirrors
 *  `ox_store::evaluation::AxisAggregate`. */
export type AxisAggregate = components["schemas"]["AxisAggregate"];

/** Run-level summary returned by the
 *  `/api/evaluation/runs/{run_id}/summary` endpoint. */
export type RunSummary = components["schemas"]["RunSummary"];

/** Mirrors `ox_store::evaluation::EvaluationDataset` —
 *  workspace-scoped, name-keyed via UPSERT so re-import
 *  preserves `id` + `created_at`. */
export type EvaluationDataset = components["schemas"]["EvaluationDataset"];

/** Dataset header + per-row aggregate. The list endpoint
 *  returns this shape so the FE renders inline item count
 *  without an N+1 fetch. */
export type EvaluationDatasetSummary = components["schemas"]["EvaluationDatasetSummary"];
export type EvaluationDatasetListPage =
  ClientPage<components["schemas"]["EvaluationDatasetSummaryPage"]>;

export type UpsertEvaluationDatasetRequest = components["schemas"]["UpsertEvaluationDatasetRequest"];

/** Mirrors `ox_store::evaluation::EvaluationDatasetItem`. */
export type EvaluationDatasetItem = components["schemas"]["EvaluationDatasetItem"];

/** One row in the `PUT /datasets/{id}/items` request body —
 *  the bulk import shape. The server stamps `id` /
 *  `dataset_id` / `workspace_id` / `created_at` so the
 *  operator-facing payload is just the editable surface. */
export type UpsertEvaluationDatasetItemEntry = components["schemas"]["UpsertEvaluationDatasetItemEntry"];
export type ReplaceEvaluationDatasetItemsRequest = components["schemas"]["ReplaceEvaluationDatasetItemsRequest"];
export type ReplaceEvaluationDatasetItemsResponse = components["schemas"]["ReplaceEvaluationDatasetItemsResponse"];

/** `POST /api/evaluation/runs/from-dataset` body. */
export type CreateRunFromDatasetRequest = components["schemas"]["CreateRunFromDatasetRequest"];
export type CreateRunFromDatasetResponse = components["schemas"]["CreateRunFromDatasetResponse"];
