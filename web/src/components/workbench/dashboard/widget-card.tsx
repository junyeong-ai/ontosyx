"use client";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
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

export function WidgetCard({ widget, selected, refreshKey, onClick }: WidgetCardProps) {
  const [queryResult, setQueryResult] = useState<QueryResult | null>(null);
  const [queryError, setQueryError] = useState<string | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  const [paramValues, setParamValues] = useState<Record<string, string | number | boolean>>(() => {
    const defaults: Record<string, string | number | boolean> = {};
    for (const p of widget.parameters ?? []) {
      if (p.default_value !== undefined) defaults[p.name] = p.default_value;
    }
    return defaults;
  });
  const intervalRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const dashboardFilters = useAppStore((s) => s.dashboardFilters);

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

  // Substitute $param placeholders in query with current parameter values
  const resolvedQuery = useMemo(() => {
    if (!widget.query) return null;
    let q = widget.query;
    for (const [name, value] of Object.entries(paramValues)) {
      const placeholder = `$${name}`;
      const replacement = typeof value === "string" ? `'${value.replace(/'/g, "\\'")}'` : String(value);
      q = q.replaceAll(placeholder, replacement);
    }
    return q;
  }, [widget.query, paramValues]);

  const executeQuery = useCallback(() => {
    if (!resolvedQuery) return;
    setRefreshing(true);
    rawQuery({ query: resolvedQuery })
      .then((r) => {
        setQueryResult(r);
        setQueryError(null);
      })
      .catch((e: unknown) => setQueryError(e instanceof Error ? e.message : "Query failed"))
      .finally(() => setRefreshing(false));
  }, [resolvedQuery]);

  // Initial query execution + refresh on refreshKey change
  useEffect(() => {
    executeQuery();
  }, [executeQuery, refreshKey]);

  // Auto-refresh interval
  useEffect(() => {
    if (intervalRef.current) {
      clearInterval(intervalRef.current);
      intervalRef.current = null;
    }
    if (widget.refresh_interval_secs && widget.refresh_interval_secs > 0 && widget.query) {
      intervalRef.current = setInterval(executeQuery, widget.refresh_interval_secs * 1000);
    }
    return () => {
      if (intervalRef.current) {
        clearInterval(intervalRef.current);
      }
    };
  }, [widget.refresh_interval_secs, widget.query, executeQuery]);

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
          {widget.refresh_interval_secs && widget.refresh_interval_secs > 0 && (
            <HugeiconsIcon
              icon={RepeatIcon}
              className={`h-3 w-3 text-zinc-400 ${refreshing ? "animate-spin" : ""}`}
              size="100%"
            />
          )}
          <span className="text-[10px] text-zinc-400">{widget.widget_type}</span>
        </div>
      </div>
      {/* Parameter inputs */}
      {widget.parameters && widget.parameters.length > 0 && (
        <div className="flex flex-wrap gap-1.5 border-b border-zinc-100 px-3 py-1.5 dark:border-zinc-800">
          {widget.parameters.map((p) => (
            <label key={p.name} className="flex items-center gap-1 text-[10px] text-zinc-500">
              <span>{p.label ?? p.name}:</span>
              <input
                type={p.type === "number" ? "number" : "text"}
                value={String(paramValues[p.name] ?? "")}
                onChange={(e) => {
                  const val = p.type === "number" ? Number(e.target.value) : e.target.value;
                  setParamValues((prev) => ({ ...prev, [p.name]: val }));
                }}
                onKeyDown={(e) => { if (e.key === "Enter") executeQuery(); }}
                className="w-20 rounded border border-zinc-200 bg-zinc-50 px-1.5 py-0.5 text-[10px] text-zinc-700 outline-none focus:border-emerald-400 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-300"
              />
            </label>
          ))}
        </div>
      )}
      <div className="p-2 min-h-[120px]">
        {queryError ? (
          <p className="text-xs text-red-500">{queryError}</p>
        ) : filteredResult ? (
          <WidgetRenderer
            spec={{ widget_type: widget.widget_type, ...widget.widget_spec } as WidgetSpec}
            data={filteredResult}
          />
        ) : widget.query ? (
          <div className="flex items-center justify-center h-full">
            <Spinner size="sm" />
          </div>
        ) : (
          <p className="text-xs text-zinc-400 text-center">No query configured</p>
        )}
      </div>
    </div>
  );
}
