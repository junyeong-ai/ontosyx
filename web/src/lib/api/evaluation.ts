import { request } from "./client";
import type {
  BulkUpsertEvaluationCasesRequest,
  BulkUpsertEvaluationCasesResponse,
  CompleteEvaluationRunRequest,
  CreateEvaluationRunRequest,
  EvaluationCase,
  EvaluationMetric,
  EvaluationRun,
  EvaluationRunListPage,
  ExecuteEvaluationCaseRequest,
  RecordEvaluationMetricRequest,
  RunComparisonReport,
  UpsertEvaluationCaseRequest,
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
