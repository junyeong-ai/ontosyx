"use client";

import type { QueryResult, WidgetSpec } from "@/types/api";
import {
  ScatterChart,
  Scatter,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
  ResponsiveContainer,
  ZAxis,
} from "recharts";
import { useIsDarkMode } from "@/lib/use-dark-mode";
import { axisTickStyle, axisLineStroke, gridStroke, tooltipStyle } from "./chart-utils";

interface ScatterChartWidgetProps {
  spec: WidgetSpec;
  data: QueryResult;
}

export function ScatterChartWidget({ spec, data }: ScatterChartWidgetProps) {
  const isDark = useIsDarkMode();
  const { columns, rows } = data;
  if (!rows.length) return <p className="text-xs text-zinc-400">No data</p>;

  // Find numeric columns for x/y/z
  const numericCols = columns.filter(
    (col) => typeof rows[0][col] === "number",
  );

  if (numericCols.length < 2)
    return (
      <p className="text-xs text-zinc-400">Need at least 2 numeric columns</p>
    );

  // Validate that spec axis fields are actually numeric columns;
  // when switching from bar/combo the inherited field may be a string column.
  const specX = spec.x_axis?.field;
  const specY = spec.y_axis?.field;
  const xKey = specX && numericCols.includes(specX) ? specX : numericCols[0];
  const yKey = specY && numericCols.includes(specY) ? specY : numericCols[1];
  const zKey = numericCols.length >= 3 ? numericCols[2] : undefined;
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
          <ScatterChart margin={{ top: 8, right: 16, bottom: 8, left: 0 }}>
            <CartesianGrid strokeDasharray="3 3" stroke={gridStroke(isDark)} />
            <XAxis
              dataKey={xKey}
              type="number"
              name={xKey}
              tick={tick}
              stroke={axisLineStroke(isDark)}
            />
            <YAxis
              dataKey={yKey}
              type="number"
              name={yKey}
              tick={tick}
              stroke={axisLineStroke(isDark)}
            />
            {zKey && (
              <ZAxis dataKey={zKey} type="number" name={zKey} range={[20, 400]} />
            )}
            <Tooltip contentStyle={tooltipStyle(isDark)} />
            <Scatter data={rows} fill="#10b981" />
          </ScatterChart>
        </ResponsiveContainer>
      </div>
      <p className="text-[10px] text-zinc-400">{rows.length} data points</p>
    </div>
  );
}
