"use client";

import { useMemo } from "react";
import { useTranslations } from "next-intl";
import { useQuery } from "@tanstack/react-query";
import { useAppStore } from "@/lib/store";
import { Repeat } from "lucide-react";
import { Spinner } from "@/components/ui/spinner";
import { WidgetRenderer } from "@/components/dashboard/widgets/widget-renderer";
import { rawQuery } from "@/lib/api";
import type { DashboardWidget, QueryResult, WidgetSpec } from "@/types/api";

export interface WidgetCardProps {
  widget: DashboardWidget;
  selected: boolean;
  refreshKey?: number;
  onClick: () => void;
}

// Stable reference so Zustand's strict-equality skip-render still
// fires when the dashboard carries no hidden types.
const EMPTY_HIDDEN: readonly string[] = Object.freeze([]);

export function WidgetCard({ widget, selected, refreshKey, onClick }: WidgetCardProps) {
  const t = useTranslations("workbench.dashboard.widget");
  const dashboardFilters = useAppStore((s) => s.dashboardFilters);

  const hiddenTypes = useAppStore((s) =>
    widget.dashboard_id
      ? s.dashboardTypeFilters[widget.dashboard_id] ?? EMPTY_HIDDEN
      : EMPTY_HIDDEN,
  );
  const clearDashboardTypes = useAppStore((s) => s.clearDashboardTypes);

  // Tanstack Query owns the execution lifecycle — initial fetch,
  // refresh-key-driven manual refetch, and interval-based
  // auto-refresh via `refetchInterval`.
  const {
    data: queryResult,
    error: queryErrorObj,
    isFetching: refreshing,
  } = useQuery<QueryResult, Error>({
    queryKey: ["widget-query", widget.id, widget.query ?? "", refreshKey ?? 0],
    queryFn: () => rawQuery({ query: widget.query as string }),
    enabled: !!widget.query,
    refetchInterval:
      widget.refresh_interval_secs && widget.refresh_interval_secs > 0
        ? widget.refresh_interval_secs * 1000
        : false,
    staleTime: Infinity,
  });
  const queryError = queryErrorObj
    ? queryErrorObj instanceof Error
      ? queryErrorObj.message
      : t("queryFailed")
    : null;

  const filteredResult = useMemo(() => {
    if (!queryResult || Object.keys(dashboardFilters).length === 0) return queryResult;
    const filtered = queryResult.rows.filter((row) =>
      Object.entries(dashboardFilters).every(([key, value]) => {
        if (!(key in row)) return true;
        return String(row[key]) === String(value);
      }),
    );
    return { ...queryResult, rows: filtered };
  }, [queryResult, dashboardFilters]);

  const pos = widget.position as { w?: number; h?: number } | undefined;
  const colSpan = Math.min(pos?.w ?? 6, 12);

  return (
    <div
      onClick={onClick}
      style={{ gridColumn: `span ${colSpan} / span ${colSpan}` }}
      className={`cursor-pointer rounded-lg border transition-all duration-[var(--duration-base)] ease-[var(--ease-out)] ${
        selected
          ? "border-brand-foreground ring-2 ring-brand-foreground/50 bg-surface-base"
          : "border-divider bg-surface-base hover:border-divider"
      }`}
    >
      <div className="flex items-center justify-between border-b border-divider-soft px-3 py-2">
        <p className="text-xs font-medium text-foreground truncate">
          {widget.title}
        </p>
        <div className="flex items-center gap-1.5">
          {hiddenTypes.length > 0 && widget.dashboard_id && (
            <button
              type="button"
              onClick={(e) => {
                e.stopPropagation();
                clearDashboardTypes(widget.dashboard_id ?? "");
              }}
              title={t("crossFilterClearTitle", { types: hiddenTypes.join(", ") })}
              className="inline-flex items-center gap-1 rounded bg-warning-surface px-1.5 py-0.5 text-2xs font-medium text-warning-foreground hover:bg-warning-surface"
            >
              <span>{t("crossFilterChip", { count: hiddenTypes.length })}</span>
              <span aria-hidden="true">×</span>
            </button>
          )}
          {widget.refresh_interval_secs && widget.refresh_interval_secs > 0 && (
            <Repeat className={`h-3 w-3 text-foreground-muted ${refreshing ? "animate-spin" : ""}`} />
          )}
          <span className="text-2xs text-foreground-muted">{widget.widget_type}</span>
        </div>
      </div>
      <div className="p-2 min-h-[120px]">
        {queryError ? (
          <p className="text-xs text-danger-foreground">{queryError}</p>
        ) : filteredResult ? (
          <WidgetRenderer
            spec={{ widget_type: widget.widget_type, ...widget.widget_spec } as WidgetSpec}
            data={filteredResult}
            dashboardId={widget.dashboard_id}
          />
        ) : widget.query ? (
          <div className="flex items-center justify-center h-full">
            <Spinner size="sm" />
          </div>
        ) : (
          <p className="text-xs text-foreground-muted text-center">{t("noQuery")}</p>
        )}
      </div>
    </div>
  );
}
