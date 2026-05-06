import { request } from "./client";
import type {
  BulkUpsertEvaluationCasesRequest,
  BulkUpsertEvaluationCasesResponse,
  CompleteEvaluationRunRequest,
  CreateEvaluationRunRequest,
  EvaluationCase,
  EvaluationDataset,
  EvaluationDatasetListPage,
  EvaluationMetric,
  EvaluationRun,
  EvaluationRunListPage,
  ExecuteEvaluationCaseRequest,
  RecordEvaluationMetricRequest,
  RunComparisonReport,
  RunSummary,
  UpsertEvaluationCaseRequest,
  UpsertEvaluationDatasetRequest,
} from "@/types/evaluation";

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
  req: ExecuteEvaluationCaseRequest,
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

/** Promote a chat-sample case into a curated dataset item. The
 *  online sampler lands cases in `live_chat_samples`; this is
 *  the operator-driven path to lift one into a regression
 *  fixture. */
export interface PromoteCaseToDatasetRequest {
  dataset_id: string;
  use_actual_as_expected?: boolean;
  item_key?: string;
}

export async function promoteCaseToDataset(
  caseId: string,
  req: PromoteCaseToDatasetRequest,
): Promise<unknown> {
  const res = await request<{ item: unknown }>(
    `/evaluation/cases/${encodeURIComponent(caseId)}/promote-to-dataset`,
    { method: "POST", body: JSON.stringify(req) },
  );
  return res.item;
}
