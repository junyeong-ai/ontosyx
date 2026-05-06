"use client";

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import {
  bulkUpsertEvaluationCases,
  cancelEvaluationRun,
  compareEvaluationRuns,
  createEvaluationRun,
  deleteEvaluationDataset,
  deleteEvaluationRun,
  executeEvaluationCase,
  getEvaluationDataset,
  getEvaluationRun,
  getEvaluationRunSummary,
  judgeEvaluationCase,
  judgeSafetyEvaluationCase,
  listEvaluationCases,
  listEvaluationDatasetItems,
  listEvaluationDatasets,
  listEvaluationMetrics,
  listEvaluationRuns,
  promoteCaseToDataset,
  upsertEvaluationDataset,
  type ListEvaluationDatasetsParams,
  type ListEvaluationRunsParams,
  type PromoteCaseToDatasetRequest,
} from "@/lib/api/evaluation";
import type {
  BulkUpsertEvaluationCasesRequest,
  BulkUpsertEvaluationCasesResponse,
  CreateEvaluationRunRequest,
  EvaluationCase,
  EvaluationDataset,
  EvaluationDatasetItem,
  EvaluationDatasetListPage,
  EvaluationMetric,
  EvaluationRun,
  EvaluationRunListPage,
  ExecuteEvaluationCaseRequest,
  RunComparisonReport,
  RunSummary,
  UpsertEvaluationDatasetRequest,
} from "@/types/evaluation";

// ---------------------------------------------------------------------------
// Query keys — `evaluationKeys.*` mirrors the API's resource shape so
// invalidations stay precise: completing a run only invalidates the
// single detail key plus the list, never the unrelated case + metric
// trees.
// ---------------------------------------------------------------------------

export const evaluationKeys = {
  all: ["evaluation"] as const,
  runs: () => [...evaluationKeys.all, "runs"] as const,
  runList: (params: ListEvaluationRunsParams) =>
    [...evaluationKeys.runs(), "list", params] as const,
  runDetail: (id: string) => [...evaluationKeys.runs(), "detail", id] as const,
  cases: (runId: string) =>
    [...evaluationKeys.all, "cases", runId] as const,
  metrics: (caseId: string) =>
    [...evaluationKeys.all, "metrics", caseId] as const,
  diff: (baselineId: string, candidateId: string) =>
    [...evaluationKeys.runs(), "diff", baselineId, candidateId] as const,
  runSummary: (id: string) =>
    [...evaluationKeys.runs(), "summary", id] as const,
  datasets: () => [...evaluationKeys.all, "datasets"] as const,
  datasetList: (params: ListEvaluationDatasetsParams) =>
    [...evaluationKeys.datasets(), "list", params] as const,
  datasetDetail: (id: string) =>
    [...evaluationKeys.datasets(), "detail", id] as const,
  datasetItems: (id: string) =>
    [...evaluationKeys.datasets(), "items", id] as const,
};

// ---------------------------------------------------------------------------
// Queries
// ---------------------------------------------------------------------------

export function useEvaluationRuns(params: ListEvaluationRunsParams = {}) {
  return useQuery<EvaluationRunListPage>({
    queryKey: evaluationKeys.runList(params),
    queryFn: () => listEvaluationRuns(params),
    // Evaluation rows are append-mostly. A 30s window is the same
    // freshness budget the dashboard uses for adjacent surfaces
    // (insights, governance approvals).
    staleTime: 30_000,
  });
}

export function useEvaluationRun(id: string | null | undefined) {
  return useQuery<EvaluationRun>({
    queryKey: evaluationKeys.runDetail(id ?? ""),
    queryFn: () => {
      if (!id) {
        throw new Error("evaluation run id is required");
      }
      return getEvaluationRun(id);
    },
    enabled: !!id,
    staleTime: 30_000,
  });
}

export function useEvaluationCases(runId: string | null | undefined) {
  return useQuery<EvaluationCase[]>({
    queryKey: evaluationKeys.cases(runId ?? ""),
    queryFn: () => {
      if (!runId) {
        throw new Error("evaluation run id is required");
      }
      return listEvaluationCases(runId);
    },
    enabled: !!runId,
    staleTime: 30_000,
  });
}

export function useEvaluationDatasets(
  params: ListEvaluationDatasetsParams = {},
) {
  return useQuery<EvaluationDatasetListPage>({
    queryKey: evaluationKeys.datasetList(params),
    queryFn: () => listEvaluationDatasets(params),
    staleTime: 30_000,
  });
}

export function useUpsertEvaluationDataset() {
  const qc = useQueryClient();
  return useMutation<EvaluationDataset, Error, UpsertEvaluationDatasetRequest>({
    mutationFn: (req) => upsertEvaluationDataset(req),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: evaluationKeys.datasets() });
    },
  });
}

export function useDeleteEvaluationDataset() {
  const qc = useQueryClient();
  return useMutation<void, Error, string>({
    mutationFn: (id) => deleteEvaluationDataset(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: evaluationKeys.datasets() });
    },
  });
}

export function useEvaluationDataset(id: string | null | undefined) {
  return useQuery<EvaluationDataset>({
    queryKey: evaluationKeys.datasetDetail(id ?? ""),
    queryFn: () => {
      if (!id) {
        throw new Error("dataset id is required");
      }
      return getEvaluationDataset(id);
    },
    enabled: !!id,
    staleTime: 30_000,
  });
}

export function useEvaluationDatasetItems(id: string | null | undefined) {
  return useQuery<EvaluationDatasetItem[]>({
    queryKey: evaluationKeys.datasetItems(id ?? ""),
    queryFn: () => {
      if (!id) {
        throw new Error("dataset id is required");
      }
      return listEvaluationDatasetItems(id);
    },
    enabled: !!id,
    staleTime: 30_000,
  });
}

/** Run-level summary (case counts + per-axis aggregate). The
 *  detail-page header card reads this; it auto-refreshes when
 *  cases / metrics around the run change because the underlying
 *  trees in `evaluationKeys.cases` / `evaluationKeys.metrics`
 *  share the same staleTime envelope. */
export function useEvaluationRunSummary(id: string | null | undefined) {
  return useQuery<RunSummary>({
    queryKey: evaluationKeys.runSummary(id ?? ""),
    queryFn: () => {
      if (!id) {
        throw new Error("evaluation run id is required");
      }
      return getEvaluationRunSummary(id);
    },
    enabled: !!id,
    staleTime: 30_000,
  });
}

/** Two-run regression diff (Phoenix/Braintrust). Disabled until
 *  both ids are present so the picker can render without firing
 *  a request prematurely. */
export function useEvaluationRunDiff(
  baselineId: string | null | undefined,
  candidateId: string | null | undefined,
) {
  return useQuery<RunComparisonReport>({
    queryKey: evaluationKeys.diff(baselineId ?? "", candidateId ?? ""),
    queryFn: () => {
      if (!baselineId || !candidateId) {
        throw new Error("baseline and candidate run ids are required");
      }
      return compareEvaluationRuns(baselineId, candidateId);
    },
    enabled: !!baselineId && !!candidateId && baselineId !== candidateId,
    staleTime: 30_000,
  });
}

// ---------------------------------------------------------------------------
// Mutations
// ---------------------------------------------------------------------------

export function useCreateEvaluationRun() {
  const qc = useQueryClient();
  return useMutation<EvaluationRun, Error, CreateEvaluationRunRequest>({
    mutationFn: (req) => createEvaluationRun(req),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: evaluationKeys.runs() });
    },
  });
}

export function useDeleteEvaluationRun() {
  const qc = useQueryClient();
  return useMutation<void, Error, string>({
    mutationFn: (id) => deleteEvaluationRun(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: evaluationKeys.runs() });
    },
  });
}

export function useCancelEvaluationRun() {
  const qc = useQueryClient();
  return useMutation<EvaluationRun, Error, string>({
    mutationFn: (id) => cancelEvaluationRun(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: evaluationKeys.runs() });
    },
  });
}

export function useJudgeEvaluationCase() {
  const qc = useQueryClient();
  return useMutation<EvaluationMetric[], Error, string>({
    mutationFn: (caseId) => judgeEvaluationCase(caseId),
    onSuccess: (_metrics, caseId) => {
      qc.invalidateQueries({ queryKey: evaluationKeys.metrics(caseId) });
    },
  });
}

/** Safety-rubric judge — runs alongside the RAGAS judge on the
 *  same case (separate `(case_id, name)` keys via `safety.*`
 *  prefix). */
export function useJudgeSafetyEvaluationCase() {
  const qc = useQueryClient();
  return useMutation<EvaluationMetric[], Error, string>({
    mutationFn: (caseId) => judgeSafetyEvaluationCase(caseId),
    onSuccess: (_metrics, caseId) => {
      qc.invalidateQueries({ queryKey: evaluationKeys.metrics(caseId) });
    },
  });
}

/** Promote a chat-sample case into a curated dataset item.
 *  Pure server-side mutation; no list-level cache invalidation
 *  needed because the dataset surface lives in its own
 *  query tree. */
export function usePromoteCaseToDataset() {
  return useMutation<
    unknown,
    Error,
    { caseId: string; request: PromoteCaseToDatasetRequest }
  >({
    mutationFn: ({ caseId, request }) =>
      promoteCaseToDataset(caseId, request),
  });
}

export function useBulkUpsertEvaluationCases(runId: string) {
  const qc = useQueryClient();
  return useMutation<
    BulkUpsertEvaluationCasesResponse,
    Error,
    BulkUpsertEvaluationCasesRequest
  >({
    mutationFn: (req) => bulkUpsertEvaluationCases(runId, req),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: evaluationKeys.cases(runId) });
    },
  });
}

export function useExecuteEvaluationCase(runId: string) {
  const qc = useQueryClient();
  return useMutation<
    EvaluationCase,
    Error,
    { caseKey: string; request: ExecuteEvaluationCaseRequest }
  >({
    mutationFn: ({ caseKey, request }) =>
      executeEvaluationCase(runId, caseKey, request),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: evaluationKeys.cases(runId) });
      // Retrying a failed case flips `failed_cases` / `total
      // judged` on the summary card; without this invalidation
      // the header tile reads stale until the next 30s
      // refetch.
      qc.invalidateQueries({ queryKey: evaluationKeys.runSummary(runId) });
    },
  });
}

export function useEvaluationMetrics(caseId: string | null | undefined) {
  return useQuery<EvaluationMetric[]>({
    queryKey: evaluationKeys.metrics(caseId ?? ""),
    queryFn: () => {
      if (!caseId) {
        throw new Error("evaluation case id is required");
      }
      return listEvaluationMetrics(caseId);
    },
    enabled: !!caseId,
    staleTime: 30_000,
  });
}
