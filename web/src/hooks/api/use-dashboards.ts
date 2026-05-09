"use client";

import {
  useMutation,
  useQuery,
  useQueryClient,
  type UseQueryOptions,
} from "@tanstack/react-query";
import {
  createDashboard,
  deleteDashboard,
  getDashboard,
  listDashboards,
  updateDashboard,
} from "@/lib/api/dashboards";
import type {
  Dashboard,
  DashboardCreateRequest,
  DashboardPage,
  DashboardUpdateRequest,
} from "@/types/api";

// ---------------------------------------------------------------------------
// Query keys
// ---------------------------------------------------------------------------

export const dashboardsKeys = {
  all: ["dashboards"] as const,
  lists: () => [...dashboardsKeys.all, "list"] as const,
  list: (params?: { limit?: number }) =>
    [...dashboardsKeys.lists(), params ?? {}] as const,
  details: () => [...dashboardsKeys.all, "detail"] as const,
  detail: (id: string) => [...dashboardsKeys.details(), id] as const,
};

// ---------------------------------------------------------------------------
// Queries
// ---------------------------------------------------------------------------

/**
 * List dashboards (single page — most callers just show a picker).
 *
 * Why not `useInfiniteQuery`: the picker UX shows a fixed top-N with `limit: 50`.
 * No "load more" interaction exists, so a flat `useQuery` is simpler.
 */
export function useDashboards(
  params?: { limit?: number },
  options?: Omit<
    UseQueryOptions<DashboardPage>,
    "queryKey" | "queryFn"
  >,
) {
  return useQuery({
    queryKey: dashboardsKeys.list(params),
    queryFn: () => listDashboards(params),
    ...options,
  });
}

export function useDashboard(
  id: string | null | undefined,
  options?: Omit<UseQueryOptions<Dashboard>, "queryKey" | "queryFn" | "enabled">,
) {
  return useQuery({
    queryKey: dashboardsKeys.detail(id ?? ""),
    queryFn: () => getDashboard(id as string),
    enabled: !!id,
    ...options,
  });
}

// ---------------------------------------------------------------------------
// Mutations
// ---------------------------------------------------------------------------

export function useCreateDashboard() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (req: DashboardCreateRequest) => createDashboard(req),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: dashboardsKeys.lists() });
    },
  });
}

export function useUpdateDashboard() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, req }: { id: string; req: DashboardUpdateRequest }) =>
      updateDashboard(id, req),
    onSuccess: (_data, { id }) => {
      qc.invalidateQueries({ queryKey: dashboardsKeys.detail(id) });
      qc.invalidateQueries({ queryKey: dashboardsKeys.lists() });
    },
  });
}

export function useDeleteDashboard() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => deleteDashboard(id),
    onSuccess: (_data, id) => {
      qc.invalidateQueries({ queryKey: dashboardsKeys.lists() });
      qc.removeQueries({ queryKey: dashboardsKeys.detail(id) });
    },
  });
}
