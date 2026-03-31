"use client";

import { cn } from "@/lib/cn";
import type { QueryResult, WidgetSpec } from "@/types/api";

// ---------------------------------------------------------------------------
// Threshold-based color for KPI values
// ---------------------------------------------------------------------------

function getThresholdColor(
  value: number,
  thresholds?: { warning?: number; critical?: number; direction?: string },
): string {
  if (!thresholds) return "text-zinc-900 dark:text-zinc-100";
  const { warning, critical, direction = "above" } = thresholds;

  if (direction === "above") {
    if (critical != null && value >= critical) return "text-red-600 dark:text-red-400";
    if (warning != null && value >= warning) return "text-amber-600 dark:text-amber-400";
  } else {
    if (critical != null && value <= critical) return "text-red-600 dark:text-red-400";
    if (warning != null && value <= warning) return "text-amber-600 dark:text-amber-400";
  }
  return "text-emerald-600 dark:text-emerald-400";
}

// ---------------------------------------------------------------------------

interface StatCardWidgetProps {
  spec: WidgetSpec;
  data: QueryResult;
}

export function StatCardWidget({ spec, data }: StatCardWidgetProps) {
  // For text widget type, just render content as markdown text
  if (spec.widget_type === "text") {
    return (
      <div className="rounded-lg border border-zinc-200 bg-white p-4 dark:border-zinc-700 dark:bg-zinc-800/60">
        {spec.title && (
          <h4 className="mb-2 text-xs font-semibold text-zinc-600 dark:text-zinc-400">
            {spec.title}
          </h4>
        )}
        <p className="text-sm text-zinc-700 dark:text-zinc-300">
          {spec.content ?? ""}
        </p>
      </div>
    );
  }

  const valueCol = spec.data_mapping?.value ?? data.columns[0];
  if (!valueCol)
    return <p className="text-xs text-zinc-400">No data available</p>;

  const labelCol = spec.data_mapping?.label as string | undefined;
  const deltaCol = spec.data_mapping?.delta as string | undefined;

  const row = data.rows[0];
  if (!row) return <p className="text-xs text-zinc-400">No data available</p>;

  const value = row[valueCol];
  const label = labelCol
    ? String(row[labelCol] ?? "")
    : spec.title ?? valueCol;
  const delta = deltaCol ? Number(row[deltaCol] ?? 0) : undefined;

  const formattedValue =
    typeof value === "number"
      ? value.toLocaleString(undefined, { maximumFractionDigits: 2 })
      : String(value ?? "\u2014");

  const thresholds = spec.thresholds as
    | { warning?: number; critical?: number; direction?: string }
    | undefined;
  const valueColor =
    typeof value === "number" && thresholds
      ? getThresholdColor(value, thresholds)
      : "text-zinc-900 dark:text-zinc-100";

  return (
    <div
      className={cn(
        "inline-flex flex-col items-center rounded-xl px-6 py-4",
        "border border-zinc-200 bg-white",
        "dark:border-zinc-700 dark:bg-zinc-800/60",
      )}
    >
      <span className={cn("text-2xl font-bold", valueColor)}>
        {formattedValue}
      </span>
      <span className="mt-1 text-xs text-zinc-500 dark:text-zinc-400">
        {label}
      </span>
      {delta !== undefined && (
        <span
          className={cn(
            "mt-1 text-xs font-medium",
            delta > 0
              ? "text-emerald-600 dark:text-emerald-400"
              : delta < 0
                ? "text-red-500 dark:text-red-400"
                : "text-zinc-400",
          )}
        >
          {delta > 0 ? "+" : ""}
          {delta}
        </span>
      )}
    </div>
  );
}
