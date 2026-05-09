import { request } from "./client";
import type {
  BulkUpsertEvaluationCasesRequest,
  BulkUpsertEvaluationCasesResponse,
  CompleteEvaluationRunRequest,
  CreateEvaluationRunRequest,
  CreateRunFromDatasetRequest,
  CreateRunFromDatasetResponse,
  EvaluationCase,
  EvaluationDataset,
  EvaluationDatasetItem,
  EvaluationDatasetListPage,
  EvaluationMetric,
  EvaluationRun,
  EvaluationRunListPage,
  EvaluationCaseInput,
  RecordEvaluationMetricRequest,
  ReplaceEvaluationDatasetItemsRequest,
  ReplaceEvaluationDatasetItemsResponse,
  RunComparisonReport,
  RunSummary,
  UpsertEvaluationCaseRequest,
  UpsertEvaluationDatasetRequest,
} from "@/types/evaluation";
import type { components } from "@/types/api.generated";

const RUNS = "/evaluation/runs";

export interface ListEvaluationRunsParams {
  cursor?: string;
  limit?: number;
}

export async function listEvaluationRuns(
  params: ListEvaluationRunsParams = {},
): Promise<EvaluationRunListPage> {
  const qs = new URLSearchParams();
  if (params.cursor) qs.set("cursor", params.cursor);
  if (params.limit) qs.set("limit", String(params.limit));
  const suffix = qs.toString() ? `?${qs}` : "";
  return request<EvaluationRunListPage>(`${RUNS}${suffix}`);
}

export async function getEvaluationRun(id: string): Promise<EvaluationRun> {
  const res = await request<{ run: EvaluationRun }>(
    `${RUNS}/${encodeURIComponent(id)}`,
  );
  return res.run;
}

export async function createEvaluationRun(
  req: CreateEvaluationRunRequest,
): Promise<EvaluationRun> {
  const res = await request<{ run: EvaluationRun }>(RUNS, {
    method: "POST",
    body: JSON.stringify(req),
  });
  return res.run;
}

export async function completeEvaluationRun(
  id: string,
  req: CompleteEvaluationRunRequest,
): Promise<EvaluationRun> {
  const res = await request<{ run: EvaluationRun }>(
    `${RUNS}/${encodeURIComponent(id)}/complete`,
    { method: "POST", body: JSON.stringify(req) },
  );
  return res.run;
}

export async function cancelEvaluationRun(id: string): Promise<EvaluationRun> {
  return completeEvaluationRun(id, { status: "cancelled" });
}

export async function deleteEvaluationRun(id: string): Promise<void> {
  await request(`${RUNS}/${encodeURIComponent(id)}`, { method: "DELETE" });
}

export async function upsertEvaluationCase(
  runId: string,
  req: UpsertEvaluationCaseRequest,
): Promise<EvaluationCase> {
  const res = await request<{ case: EvaluationCase }>(
    `${RUNS}/${encodeURIComponent(runId)}/cases`,
    { method: "PUT", body: JSON.stringify(req) },
  );
  return res.case;
}

export async function listEvaluationCases(
  runId: string,
): Promise<EvaluationCase[]> {
  return request<EvaluationCase[]>(
    `${RUNS}/${encodeURIComponent(runId)}/cases`,
  );
}

export async function bulkUpsertEvaluationCases(
  runId: string,
  req: BulkUpsertEvaluationCasesRequest,
): Promise<BulkUpsertEvaluationCasesResponse> {
  return request<BulkUpsertEvaluationCasesResponse>(
    `${RUNS}/${encodeURIComponent(runId)}/cases/bulk`,
    { method: "POST", body: JSON.stringify(req) },
  );
}

export async function executeEvaluationCase(
  runId: string,
  caseKey: string,
  req: EvaluationCaseInput,
): Promise<EvaluationCase> {
  const res = await request<{ case: EvaluationCase }>(
    `${RUNS}/${encodeURIComponent(runId)}/cases/${encodeURIComponent(caseKey)}/execute`,
    { method: "POST", body: JSON.stringify(req) },
  );
  return res.case;
}

export async function recordEvaluationMetric(
  caseId: string,
  req: RecordEvaluationMetricRequest,
): Promise<EvaluationMetric> {
  const res = await request<{ metric: EvaluationMetric }>(
    `/evaluation/cases/${encodeURIComponent(caseId)}/metrics`,
    { method: "PUT", body: JSON.stringify(req) },
  );
  return res.metric;
}

export async function listEvaluationMetrics(
  caseId: string,
): Promise<EvaluationMetric[]> {
  return request<EvaluationMetric[]>(
    `/evaluation/cases/${encodeURIComponent(caseId)}/metrics`,
  );
}

export async function judgeEvaluationCase(
  caseId: string,
): Promise<EvaluationMetric[]> {
  const res = await request<{ metrics: EvaluationMetric[] }>(
    `/evaluation/cases/${encodeURIComponent(caseId)}/judge`,
    { method: "POST" },
  );
  return res.metrics;
}

/** Safety-axis judge — toxicity / PII / factual / harmfulness.
 *  Distinct from the RAGAS judge; both can run on the same
 *  case without metric-row collisions because the safety axes
 *  ride a `safety.*` name prefix. */
export async function judgeSafetyEvaluationCase(
  caseId: string,
): Promise<EvaluationMetric[]> {
  const res = await request<{ metrics: EvaluationMetric[] }>(
    `/evaluation/cases/${encodeURIComponent(caseId)}/judge_safety`,
    { method: "POST" },
  );
  return res.metrics;
}

/** Run summary — case counts + per-axis aggregate in one
 *  round trip. Drives the run-detail header card and (future)
 *  run-list badge so operators triage without drilling into
 *  every per-case + per-metric list. */
export async function getEvaluationRunSummary(
  runId: string,
): Promise<RunSummary> {
  return request<RunSummary>(
    `${RUNS}/${encodeURIComponent(runId)}/summary`,
  );
}

/** Worst-first list of case-level retrieval-comparison
 *  outliers in a run. Drives the dashboard's per-cell drill
 *  down — the operator clicks a (surface, axis) cell whose
 *  `mean_lift` is low and the response surfaces the bad-actor
 *  cases dragging the average down. */
export interface ComparisonOutliersParams {
  surface?: "verified_query" | "community_summary" | "knowledge_entry";
  axis?: string;
  limit?: number;
}

export async function listRunComparisonOutliers(
  runId: string,
  params: ComparisonOutliersParams = {},
): Promise<components["schemas"]["ComparisonOutliersResponse"]> {
  const qs = new URLSearchParams();
  if (params.surface) qs.set("surface", params.surface);
  if (params.axis) qs.set("axis", params.axis);
  if (params.limit) qs.set("limit", String(params.limit));
  const query = qs.toString();
  return request(
    `${RUNS}/${encodeURIComponent(runId)}/comparison-outliers${
      query ? `?${query}` : ""
    }`,
  );
}

/** Read this workspace's evaluation settings. Missing fields
 *  resolve to platform defaults server-side; the FE renders
 *  the same form whether or not the operator has overridden. */
export async function getEvaluationSettings(): Promise<
  components["schemas"]["WorkspaceEvaluationSettings"]
> {
  return request("/evaluation/settings");
}

/** Admin-gated update of this workspace's evaluation settings.
 *  Validation runs server-side; an invalid threshold returns
 *  a typed `validation` error envelope. Other settings keys on
 *  `workspaces.settings` round-trip unchanged (jsonb_set
 *  partial update). */
export async function updateEvaluationSettings(
  body: components["schemas"]["WorkspaceEvaluationSettings"],
): Promise<components["schemas"]["WorkspaceEvaluationSettings"]> {
  return request("/evaluation/settings", {
    method: "PUT",
    body: JSON.stringify(body),
  });
}

/** Diff two runs over the same dataset. Backend is the
 *  `compare_evaluation_runs` Phoenix/Braintrust-style report —
 *  per-case delta rows + per-axis aggregate (mean delta,
 *  win-rate, Cohen's d). */
export async function compareEvaluationRuns(
  baselineId: string,
  candidateId: string,
): Promise<RunComparisonReport> {
  const qs = new URLSearchParams();
  qs.set("baseline", baselineId);
  qs.set("candidate", candidateId);
  return request<RunComparisonReport>(
    `${RUNS}/diff?${qs.toString()}`,
  );
}

const DATASETS = "/evaluation/datasets";

export interface ListEvaluationDatasetsParams {
  cursor?: string;
  limit?: number;
}

export async function listEvaluationDatasets(
  params: ListEvaluationDatasetsParams = {},
): Promise<EvaluationDatasetListPage> {
  const qs = new URLSearchParams();
  if (params.cursor) qs.set("cursor", params.cursor);
  if (params.limit) qs.set("limit", String(params.limit));
  const suffix = qs.toString() ? `?${qs}` : "";
  return request<EvaluationDatasetListPage>(`${DATASETS}${suffix}`);
}

/** Insert-or-update a dataset on `(workspace_id, name)`. The
 *  natural-key UPSERT preserves `id` + `created_at` on re-import
 *  under the same name; just `description` rolls forward. */
export async function upsertEvaluationDataset(
  req: UpsertEvaluationDatasetRequest,
): Promise<EvaluationDataset> {
  const res = await request<{ dataset: EvaluationDataset }>(DATASETS, {
    method: "POST",
    body: JSON.stringify(req),
  });
  return res.dataset;
}

export async function deleteEvaluationDataset(id: string): Promise<void> {
  await request(`${DATASETS}/${encodeURIComponent(id)}`, {
    method: "DELETE",
  });
}

export async function getEvaluationDataset(
  id: string,
): Promise<EvaluationDataset> {
  const res = await request<{ dataset: EvaluationDataset }>(
    `${DATASETS}/${encodeURIComponent(id)}`,
  );
  return res.dataset;
}

export async function listEvaluationDatasetItems(
  datasetId: string,
): Promise<EvaluationDatasetItem[]> {
  return request<EvaluationDatasetItem[]>(
    `${DATASETS}/${encodeURIComponent(datasetId)}/items`,
  );
}

/** Materialise a fresh run from an existing dataset. The
 *  server clones every dataset item into an EvaluationCase
 *  under the new run, with `case_key = item_key`. Atomic:
 *  failed materialisation rolls the whole run back. */
export async function createRunFromDataset(
  req: CreateRunFromDatasetRequest,
): Promise<CreateRunFromDatasetResponse> {
  return request<CreateRunFromDatasetResponse>(
    `${RUNS}/from-dataset`,
    { method: "POST", body: JSON.stringify(req) },
  );
}

/** Atomic replace — items in the body land via UPSERT on
 *  `(dataset_id, item_key)`; items in the DB but missing from
 *  the body are deleted. Use carefully — passing an empty
 *  list clears the dataset. */
export async function replaceEvaluationDatasetItems(
  datasetId: string,
  req: ReplaceEvaluationDatasetItemsRequest,
): Promise<ReplaceEvaluationDatasetItemsResponse> {
  return request<ReplaceEvaluationDatasetItemsResponse>(
    `${DATASETS}/${encodeURIComponent(datasetId)}/items`,
    { method: "PUT", body: JSON.stringify(req) },
  );
}

export type PromoteCaseToDatasetRequest =
  components["schemas"]["PromoteCaseToDatasetRequest"];
export type PromoteCaseToDatasetResponse =
  components["schemas"]["PromoteCaseToDatasetResponse"];

export async function promoteCaseToDataset(
  caseId: string,
  req: PromoteCaseToDatasetRequest,
): Promise<PromoteCaseToDatasetResponse["item"]> {
  const res = await request<PromoteCaseToDatasetResponse>(
    `/evaluation/cases/${encodeURIComponent(caseId)}/promote-to-dataset`,
    { method: "POST", body: JSON.stringify(req) },
  );
  return res.item;
}
