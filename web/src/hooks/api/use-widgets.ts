"use client";

import {
  useMutation,
  useQuery,
  useQueryClient,
  type UseQueryOptions,
} from "@tanstack/react-query";
import {
  addWidget,
  deleteWidget,
  listWidgets,
  updateWidget,
} from "@/lib/api/dashboards";
import type {
  DashboardWidget,
  WidgetCreateRequest,
  WidgetUpdateRequest,
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

export function useUpdateWidget() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({
      dashboardId,
      widgetId,
      req,
    }: {
      dashboardId: string;
      widgetId: string;
      req: WidgetUpdateRequest;
    }) => updateWidget(dashboardId, widgetId, req),
    onSuccess: (_data, { dashboardId }) => {
      // Widget config changes (query, thresholds, refresh interval)
      // shape downstream rendering — bust the list cache so every
      // open dashboard re-reads. The per-widget data query keys on
      // `widget.query` already, so a query change reruns the fetch
      // automatically once the new widget object lands.
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
