"use client";

import { useMemo } from "react";
import type { QueryResult, WidgetSpec } from "@/types/api";
import {
  ComposedChart,
  Bar,
  Line,
  XAxis,
  YAxis,
  Tooltip,
  ResponsiveContainer,
  CartesianGrid,
  Legend,
} from "recharts";
import { useIsDarkMode } from "@/lib/use-dark-mode";
import {
  PALETTE_PRIMARY,
  PALETTE_SECONDARY,
  CATEGORY_THRESHOLD,
  MAX_BAR_SIZE,
  resolveLabelField,
  formatValue,
  axisTickStyle,
  axisLineStroke,
  gridStroke,
  tooltipStyle,
} from "./chart-utils";

interface ComboChartWidgetProps {
  spec: WidgetSpec;
  data: QueryResult;
}

/**
 * Detect numeric columns excluding the label column.
 */
function getNumericColumns(data: QueryResult, labelCol: string): string[] {
  if (data.rows.length === 0) return [];
  const first = data.rows[0];
  return data.columns.filter(
    (col) => col !== labelCol && typeof first[col] === "number",
  );
}

/**
 * Analyze scale difference across numeric columns.
 * Returns { mixed: true, lineIndex } if scales differ significantly (ratio > 10).
 * lineIndex = the column with the largest max value (→ right Y axis as line).
 */
function analyzeScales(
  data: QueryResult,
  numericCols: string[],
): { mixed: boolean; lineIndex: number } {
  if (numericCols.length < 2) return { mixed: false, lineIndex: -1 };
  const maxValues = numericCols.map((col) =>
    Math.max(...data.rows.map((r) => Math.abs(Number(r[col] ?? 0)))),
  );
  const max = Math.max(...maxValues);
  const min = Math.min(...maxValues.filter((v) => v > 0));
  const mixed = min > 0 && max / min > 10;
  const lineIndex = mixed ? maxValues.indexOf(max) : -1;
  return { mixed, lineIndex };
}

export function ComboChartWidget({ spec, data }: ComboChartWidgetProps) {
  const isDark = useIsDarkMode();
  const labelCol = resolveLabelField(spec, data);
  const numericCols = useMemo(
    () => (labelCol ? getNumericColumns(data, labelCol) : []),
    [data, labelCol],
  );

  const { mixed, lineIndex } = useMemo(
    () => analyzeScales(data, numericCols),
    [data, numericCols],
  );

  // Bar = all cols except the highest-scale one; Line = highest-scale col (right Y)
  const barCols = mixed
    ? numericCols.filter((_, i) => i !== lineIndex)
    : numericCols;
  const lineCols = mixed ? [numericCols[lineIndex]] : [];

  const chartData = useMemo(
    () =>
      labelCol
        ? data.rows.map((row) => {
            const entry: Record<string, unknown> = {
              name: String(row[labelCol] ?? ""),
            };
            for (const col of numericCols) {
              entry[col] = Number(row[col] ?? 0);
            }
            return entry;
          })
        : [],
    [data.rows, labelCol, numericCols],
  );

  if (!labelCol || numericCols.length < 2 || chartData.length === 0) {
    return (
      <p className="text-xs text-zinc-400">
        Combo chart requires a label column and 2+ numeric columns
      </p>
    );
  }

  const rotated = chartData.length > CATEGORY_THRESHOLD;
  const tick = axisTickStyle(isDark);

  return (
    <div className="space-y-2">
      {spec.title && (
        <h4 className="text-xs font-semibold text-zinc-600 dark:text-zinc-400">
          {spec.title}
        </h4>
      )}
      <div className="h-72 w-full">
        <ResponsiveContainer width="100%" height="100%">
          <ComposedChart
            data={chartData}
            margin={{ top: 4, right: mixed ? 16 : 8, left: 0, bottom: 4 }}
          >
            <CartesianGrid strokeDasharray="3 3" stroke={gridStroke(isDark)} />
            <XAxis
              dataKey="name"
              tick={tick}
              axisLine={{ stroke: axisLineStroke(isDark) }}
              tickLine={false}
              interval={0}
              angle={rotated ? -45 : 0}
              textAnchor={rotated ? "end" : "middle"}
              height={rotated ? 60 : 30}
            />

            {/* Left Y axis (bars) */}
            <YAxis
              yAxisId="left"
              tick={tick}
              axisLine={false}
              tickLine={false}
              width={55}
              tickFormatter={(v: number) => v.toLocaleString()}
            />

            {/* Right Y axis (lines) — only if mixed scale */}
            {mixed && (
              <YAxis
                yAxisId="right"
                orientation="right"
                tick={{ ...tick, fill: PALETTE_SECONDARY[0] }}
                axisLine={false}
                tickLine={false}
                width={65}
                tickFormatter={(v: number) => v.toLocaleString()}
              />
            )}

            <Tooltip
              contentStyle={tooltipStyle(isDark)}
              formatter={(value: unknown, name: unknown) => [
                formatValue(value),
                String(name ?? ""),
              ]}
            />

            <Legend
              wrapperStyle={{ fontSize: 11, color: isDark ? "#a1a1aa" : "#71717a" }}
              iconType="circle"
              iconSize={8}
            />

            {/* Bar series */}
            {barCols.map((col, i) => (
              <Bar
                key={col}
                yAxisId="left"
                dataKey={col}
                name={col}
                fill={PALETTE_PRIMARY[i % PALETTE_PRIMARY.length]}
                radius={[3, 3, 0, 0]}
                maxBarSize={MAX_BAR_SIZE}
              />
            ))}

            {/* Line series (right Y axis if mixed) */}
            {lineCols.map((col, i) => (
              <Line
                key={col}
                yAxisId={mixed ? "right" : "left"}
                type="monotone"
                dataKey={col}
                name={col}
                stroke={PALETTE_SECONDARY[i % PALETTE_SECONDARY.length]}
                strokeWidth={2}
                dot={{ r: 3, fill: PALETTE_SECONDARY[i % PALETTE_SECONDARY.length] }}
                activeDot={{ r: 5 }}
              />
            ))}
          </ComposedChart>
        </ResponsiveContainer>
      </div>
      <p className="text-[10px] text-zinc-400">
        {chartData.length} items · {barCols.length} bar
        {barCols.length > 1 ? "s" : ""}
        {lineCols.length > 0
          ? ` + ${lineCols.length} line${lineCols.length > 1 ? "s" : ""}`
          : ""}
        {mixed ? " (dual axis)" : " (grouped)"}
      </p>
    </div>
  );
}
