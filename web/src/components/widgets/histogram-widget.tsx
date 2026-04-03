"use client";

import { useMemo } from "react";
import type { QueryResult, WidgetSpec } from "@/types/api";
import {
  BarChart,
  Bar,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
  ResponsiveContainer,
} from "recharts";
import { useIsDarkMode } from "@/lib/use-dark-mode";
import { axisTickStyle, axisLineStroke, gridStroke, tooltipStyle } from "./chart-utils";

interface HistogramWidgetProps {
  spec: WidgetSpec;
  data: QueryResult;
}

export function HistogramWidget({ spec, data }: HistogramWidgetProps) {
  const isDark = useIsDarkMode();
  const { columns, rows } = data;

  const binData = useMemo(() => {
    if (!rows.length) return [];

    // Find first numeric column
    const numCol = columns.find((col) => typeof rows[0][col] === "number");
    if (!numCol) return [];

    const values = rows
      .map((r) => r[numCol] as number)
      .filter((v) => v != null && !isNaN(v));

    if (values.length === 0) return [];

    const min = Math.min(...values);
    const max = Math.max(...values);
    if (min === max) return [{ bin: String(min), count: values.length }];

    // Automatic binning (max 20 bins)
    const binCount = Math.min(20, Math.ceil(Math.sqrt(values.length)));
    const binWidth = (max - min) / binCount;

    const bins = Array.from({ length: binCount }, (_, i) => ({
      bin: `${(min + i * binWidth).toFixed(1)}`,
      min: min + i * binWidth,
      max: min + (i + 1) * binWidth,
      count: 0,
    }));

    for (const v of values) {
      const idx = Math.min(Math.floor((v - min) / binWidth), binCount - 1);
      bins[idx].count++;
    }

    return bins;
  }, [columns, rows]);

  if (!binData.length)
    return <p className="text-xs text-zinc-400">No numeric data</p>;

  const tick = axisTickStyle(isDark);

  return (
    <div className="space-y-2">
      {spec.title && (
        <h4 className="text-xs font-semibold text-zinc-600 dark:text-zinc-400">
          {spec.title}
        </h4>
      )}
      <div className="h-64 w-full overflow-hidden">
        <ResponsiveContainer width="100%" height="100%">
          <BarChart
            data={binData}
            margin={{ top: 8, right: 16, bottom: 8, left: 0 }}
          >
            <CartesianGrid strokeDasharray="3 3" stroke={gridStroke(isDark)} />
            <XAxis
              dataKey="bin"
              tick={tick}
              stroke={axisLineStroke(isDark)}
            />
            <YAxis tick={tick} stroke={axisLineStroke(isDark)} />
            <Tooltip contentStyle={tooltipStyle(isDark)} />
            <Bar dataKey="count" fill="#10b981" radius={[2, 2, 0, 0]} />
          </BarChart>
        </ResponsiveContainer>
      </div>
      <p className="text-[10px] text-zinc-400">{binData.length} bins</p>
    </div>
  );
}
