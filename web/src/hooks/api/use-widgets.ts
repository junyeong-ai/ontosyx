"use client";

import {
  useMutation,
  useQuery,
  useQueryClient,
  type UseQueryOptions,
} from "@tanstack/react-query";
import { addWidget, deleteWidget, listWidgets } from "@/lib/api/dashboards";
import type {
  DashboardWidget,
  WidgetCreateRequest,
} from "@/types/api";

// ---------------------------------------------------------------------------
// Query keys
// ---------------------------------------------------------------------------

export const widgetsKeys = {
  all: ["widgets"] as const,
  lists: () => [...widgetsKeys.all, "list"] as const,
  list: (dashboardId: string) => [...widgetsKeys.lists(), dashboardId] as const,
};

// ---------------------------------------------------------------------------
// Queries
// ---------------------------------------------------------------------------

export function useWidgets(
  dashboardId: string | null | undefined,
  options?: Omit<
    UseQueryOptions<DashboardWidget[]>,
    "queryKey" | "queryFn" | "enabled"
  >,
) {
  return useQuery({
    queryKey: widgetsKeys.list(dashboardId ?? ""),
    queryFn: () => listWidgets(dashboardId as string),
    enabled: !!dashboardId,
    ...options,
  });
}

// ---------------------------------------------------------------------------
// Mutations
// ---------------------------------------------------------------------------

export function useAddWidget() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({
      dashboardId,
      req,
    }: {
      dashboardId: string;
      req: WidgetCreateRequest;
    }) => addWidget(dashboardId, req),
    onSuccess: (_data, { dashboardId }) => {
      qc.invalidateQueries({ queryKey: widgetsKeys.list(dashboardId) });
    },
  });
}

export function useDeleteWidget() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({
      dashboardId,
      widgetId,
    }: {
      dashboardId: string;
      widgetId: string;
    }) => deleteWidget(dashboardId, widgetId),
    onSuccess: (_data, { dashboardId }) => {
      qc.invalidateQueries({ queryKey: widgetsKeys.list(dashboardId) });
    },
  });
}
