"use client";

import { useMemo } from "react";
import { useTranslations } from "next-intl";
import { useQuery } from "@tanstack/react-query";
import { useAppStore } from "@/lib/store";
import { HugeiconsIcon } from "@hugeicons/react";
import { RepeatIcon } from "@hugeicons/core-free-icons";
import { Spinner } from "@/components/ui/spinner";
import { WidgetRenderer } from "@/components/widgets/widget-renderer";
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
const EMPTY_HIDDEN: readonly string[] = Object.freeze([]) as unknown as readonly string[];

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
      className={`cursor-pointer rounded-lg border transition-all ${
        selected
          ? "border-emerald-500 ring-2 ring-emerald-500/50 bg-white dark:bg-zinc-950"
          : "border-zinc-200 bg-white hover:border-zinc-300 dark:border-zinc-800 dark:bg-zinc-950 dark:hover:border-zinc-700"
      }`}
    >
      <div className="flex items-center justify-between border-b border-zinc-100 px-3 py-2 dark:border-zinc-800">
        <p className="text-xs font-medium text-zinc-700 dark:text-zinc-300 truncate">
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
              className="inline-flex items-center gap-1 rounded bg-amber-100 px-1.5 py-0.5 text-[10px] font-medium text-amber-700 hover:bg-amber-200 dark:bg-amber-950/40 dark:text-amber-300 dark:hover:bg-amber-900/40"
            >
              <span>{t("crossFilterChip", { count: hiddenTypes.length })}</span>
              <span aria-hidden="true">×</span>
            </button>
          )}
          {widget.refresh_interval_secs && widget.refresh_interval_secs > 0 && (
            <HugeiconsIcon
              icon={RepeatIcon}
              className={`h-3 w-3 text-muted-foreground ${refreshing ? "animate-spin" : ""}`}
              size="100%"
            />
          )}
          <span className="text-[10px] text-muted-foreground">{widget.widget_type}</span>
        </div>
      </div>
      <div className="p-2 min-h-[120px]">
        {queryError ? (
          <p className="text-xs text-red-500">{queryError}</p>
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
          <p className="text-xs text-muted-foreground text-center">{t("noQuery")}</p>
        )}
      </div>
    </div>
  );
}
