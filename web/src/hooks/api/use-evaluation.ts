"use client";

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import {
  executeEvaluationCase,
  getEvaluationRun,
  listEvaluationCases,
  listEvaluationMetrics,
  listEvaluationRuns,
  type ListEvaluationRunsParams,
} from "@/lib/api/evaluation";
import type {
  EvaluationCase,
  EvaluationMetric,
  EvaluationRun,
  EvaluationRunListPage,
  ExecuteEvaluationCaseRequest,
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
