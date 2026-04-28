"use client";

import {
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";

import {
  createInsight,
  deleteInsight,
  getInsight,
  listInsights,
  type ListInsightsParams,
  updateInsight,
} from "@/lib/api/insights";
import type {
  CreateInsightRequest,
  InsightDef,
  InsightListPage,
  UpdateInsightRequest,
} from "@/types/api";

// ---------------------------------------------------------------------------
// Query keys
// ---------------------------------------------------------------------------

export const insightsKeys = {
  all: ["insights"] as const,
  list: (params: ListInsightsParams) =>
    [...insightsKeys.all, "list", params] as const,
  detail: (id: string) => [...insightsKeys.all, "detail", id] as const,
};

// ---------------------------------------------------------------------------
// Queries
// ---------------------------------------------------------------------------

export function useInsights(params: ListInsightsParams = {}) {
  return useQuery<InsightListPage>({
    queryKey: insightsKeys.list(params),
    queryFn: () => listInsights(params),
    // Insights are user-curated and read more often than written;
    // a 30s window stays fresh enough for "I just saved one" without
    // hammering the backend on dashboard scroll.
    staleTime: 30_000,
  });
}

export function useInsight(id: string | null | undefined) {
  return useQuery<InsightDef>({
    queryKey: insightsKeys.detail(id ?? ""),
    queryFn: () => getInsight(id as string),
    enabled: !!id,
  });
}

// ---------------------------------------------------------------------------
// Mutations
// ---------------------------------------------------------------------------

export function useCreateInsight() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (req: CreateInsightRequest) => createInsight(req),
    onSuccess: () => {
      // Invalidate the entire `insights` family — the new row may
      // appear in any (`me=true`/`me=false`, cursor=...) variant.
      qc.invalidateQueries({ queryKey: insightsKeys.all });
    },
  });
}

export function useUpdateInsight() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, req }: { id: string; req: UpdateInsightRequest }) =>
      updateInsight(id, req),
    onSuccess: (_data, vars) => {
      qc.invalidateQueries({ queryKey: insightsKeys.detail(vars.id) });
      qc.invalidateQueries({ queryKey: insightsKeys.all });
    },
  });
}

export function useDeleteInsight() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => deleteInsight(id),
    onSuccess: (_data, id) => {
      qc.invalidateQueries({ queryKey: insightsKeys.detail(id) });
      qc.invalidateQueries({ queryKey: insightsKeys.all });
    },
  });
}
