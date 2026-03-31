"use client";

import { useMemo } from "react";
import { cn } from "@/lib/cn";
import type { QueryResult, WidgetSpec } from "@/types/api";
import { PALETTE_PRIMARY } from "./chart-utils";

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
    return <p className="text-xs text-zinc-400">Need stage and value columns for funnel</p>;
  }

  const maxValue = Math.max(...stages.map((s) => s.value), 1);

  return (
    <div className="space-y-2">
      {spec.title && (
        <h4 className="text-xs font-semibold text-zinc-600 dark:text-zinc-400">
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
                  <div className="mb-0.5 text-[10px] font-medium text-zinc-400 dark:text-zinc-500">
                    {conversionRate}%
                  </div>
                )}
                {/* Bar */}
                <div
                  className="mx-auto flex items-center justify-center rounded-md py-2 transition-all"
                  style={{
                    width: `${widthPct}%`,
                    backgroundColor: color,
                    minHeight: 32,
                  }}
                >
                  <span className="truncate px-2 text-[11px] font-semibold text-white">
                    {stage.name}
                  </span>
                </div>
              </div>
              {/* Value label */}
              <div className="w-16 shrink-0 text-right">
                <span className="text-xs font-medium text-zinc-700 dark:text-zinc-300">
                  {stage.value.toLocaleString()}
                </span>
              </div>
            </div>
          );
        })}
      </div>
      <p className="text-[10px] text-zinc-400">{stages.length} stages</p>
    </div>
  );
}
