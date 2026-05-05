"use client";

import { useMemo } from "react";
import { useTranslations } from "next-intl";
import type { QueryResult, WidgetSpec } from "@/types/api";
import { PALETTE_PRIMARY } from "./chart-utils";
import { useFormatters } from "@/hooks/use-formatters";

interface FunnelWidgetProps {
  spec: WidgetSpec;
  data: QueryResult;
}

const STAGE_PATTERNS = ["stage", "name", "step", "label", "phase", "status"];
const VALUE_PATTERNS = ["value", "count", "total", "amount", "users", "sessions"];

function findColumn(columns: string[], patterns: string[], rows: QueryResult["rows"], preferType?: "string" | "number"): string | undefined {
  const lower = columns.map((c) => c.toLowerCase());
  for (const pat of patterns) {
    const idx = lower.findIndex((c) => c === pat || c.includes(pat));
    if (idx >= 0) return columns[idx];
  }
  // Fallback: first column of preferred type
  if (preferType && rows.length > 0) {
    for (const col of columns) {
      if (typeof rows[0][col] === preferType) return col;
    }
  }
  return undefined;
}

export function FunnelWidget({ spec, data }: FunnelWidgetProps) {
  const t = useTranslations("widget.funnel");
  const fmt = useFormatters();
  const { columns, rows } = data;

  const stageCol = useMemo(
    () => spec.data_mapping?.label ?? findColumn(columns, STAGE_PATTERNS, rows, "string") ?? columns[0],
    [spec, columns, rows],
  );
  const valueCol = useMemo(
    () => spec.data_mapping?.value ?? findColumn(columns, VALUE_PATTERNS, rows, "number") ?? columns[1],
    [spec, columns, rows],
  );

  const stages = useMemo(() => {
    if (!stageCol || !valueCol) return [];
    return rows.map((row) => ({
      name: String(row[stageCol] ?? ""),
      value: Number(row[valueCol] ?? 0),
    }));
  }, [rows, stageCol, valueCol]);

  if (!stageCol || !valueCol || stages.length === 0) {
    return <p className="text-xs text-foreground-muted">{t("needColumns")}</p>;
  }

  const maxValue = Math.max(...stages.map((s) => s.value), 1);

  return (
    <div className="space-y-2">
      {spec.title && (
        <h4 className="text-xs font-semibold text-foreground">
          {spec.title}
        </h4>
      )}
      <div className="space-y-1">
        {stages.map((stage, i) => {
          const widthPct = Math.max((stage.value / maxValue) * 100, 8);
          const prevValue = i > 0 ? stages[i - 1].value : null;
          const conversionRate =
            prevValue && prevValue > 0
              ? ((stage.value / prevValue) * 100).toFixed(1)
              : null;
          const color = PALETTE_PRIMARY[i % PALETTE_PRIMARY.length];

          return (
            <div key={i} className="flex items-center gap-2">
              <div className="flex flex-1 flex-col items-center">
                {/* Conversion arrow */}
                {conversionRate && (
                  <div className="mb-0.5 text-2xs font-medium text-foreground-muted">
                    {conversionRate}%
                  </div>
                )}
                {/* Bar */}
                <div
                  className="mx-auto flex items-center justify-center rounded-md py-2 transition-all duration-[var(--duration-base)] ease-[var(--ease-out)]"
                  style={{
                    width: `${widthPct}%`,
                    backgroundColor: color,
                    minHeight: 32,
                  }}
                >
                  <span className="truncate px-2 text-2xs font-semibold text-foreground-onbrand">
                    {stage.name}
                  </span>
                </div>
              </div>
              {/* Value label */}
              <div className="w-16 shrink-0 text-end">
                <span className="text-xs font-medium text-foreground">
                  {fmt.number(stage.value)}
                </span>
              </div>
            </div>
          );
        })}
      </div>
      <p className="text-2xs text-foreground-muted">{t("stagesCount", { count: stages.length })}</p>
    </div>
  );
}
