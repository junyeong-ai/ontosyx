"use client";

import { useTranslations } from "next-intl";
import { cn } from "@/lib/cn";
import { Card } from "@/components/ui/card";
import type { QueryResult, WidgetSpec } from "@/types/api";

// ---------------------------------------------------------------------------
// Threshold-based color for KPI values
// ---------------------------------------------------------------------------

function getThresholdColor(
  value: number,
  thresholds?: { warning?: number; critical?: number; direction?: string },
): string {
  if (!thresholds) return "text-foreground-strong";
  const { warning, critical, direction = "above" } = thresholds;

  if (direction === "above") {
    if (critical != null && value >= critical) return "text-danger-foreground";
    if (warning != null && value >= warning) return "text-warning-foreground";
  } else {
    if (critical != null && value <= critical) return "text-danger-foreground";
    if (warning != null && value <= warning) return "text-warning-foreground";
  }
  return "text-brand-foreground";
}

// ---------------------------------------------------------------------------

interface StatCardWidgetProps {
  spec: WidgetSpec;
  data: QueryResult;
}

export function StatCardWidget({ spec, data }: StatCardWidgetProps) {
  const t = useTranslations("widget.statCard");
  // For text widget type, just render content as markdown text
  if (spec.widget_type === "text") {
    return (
      <Card padding="md">
        {spec.title && (
          <h4 className="mb-2 text-xs font-semibold text-foreground-muted">
            {spec.title}
          </h4>
        )}
        <p className="text-sm text-foreground">
          {spec.content ?? ""}
        </p>
      </Card>
    );
  }

  const valueCol = spec.data_mapping?.value ?? data.columns[0];
  if (!valueCol)
    return <p className="text-xs text-muted-foreground">{t("noData")}</p>;

  const labelCol = spec.data_mapping?.label as string | undefined;
  const deltaCol = spec.data_mapping?.delta as string | undefined;

  const row = data.rows[0];
  if (!row) return <p className="text-xs text-muted-foreground">{t("noData")}</p>;

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
      : "text-foreground-strong";

  return (
    <div
      className={cn(
        "inline-flex flex-col items-center rounded-xl px-6 py-4",
        "border border-divider bg-surface-base",
        "dark:border-divider",
      )}
    >
      <span className={cn("text-2xl font-bold", valueColor)}>
        {formattedValue}
      </span>
      <span className="mt-1 text-xs text-foreground-muted">
        {label}
      </span>
      {delta !== undefined && (
        <span
          className={cn(
            "mt-1 text-xs font-medium",
            delta > 0
              ? "text-brand-foreground"
              : delta < 0
                ? "text-danger-foreground"
                : "text-muted-foreground",
          )}
        >
          {delta > 0 ? "+" : ""}
          {delta}
        </span>
      )}
    </div>
  );
}
