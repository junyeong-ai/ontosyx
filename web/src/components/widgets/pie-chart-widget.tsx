"use client";

import { useMemo } from "react";
import type { QueryResult, WidgetSpec } from "@/types/api";
import type { PieLabelRenderProps } from "recharts";
import {
  PieChart,
  Pie,
  Cell,
  Tooltip,
  ResponsiveContainer,
  Legend,
} from "recharts";
import { useAppStore } from "@/lib/store";
import { useIsDarkMode } from "@/lib/use-dark-mode";
import {
  PALETTE_PRIMARY,
  CATEGORY_THRESHOLD,
  resolveLabelField,
  resolveValueField,
  toNameValuePairs,
  tooltipStyle,
  pieLabelFill,
  pieLabelLineStroke,
} from "./chart-utils";

interface PieChartWidgetProps {
  spec: WidgetSpec;
  data: QueryResult;
}

export function PieChartWidget({ spec, data }: PieChartWidgetProps) {
  const isDark = useIsDarkMode();
  const labelField = resolveLabelField(spec, data);
  const valueField = resolveValueField(spec, data);

  const chartData = useMemo(
    () =>
      labelField && valueField
        ? toNameValuePairs(data.rows, labelField, valueField)
        : [],
    [data.rows, labelField, valueField],
  );

  const total = useMemo(
    () => chartData.reduce((s, d) => s + d.value, 0),
    [chartData],
  );

  if (!labelField || !valueField || chartData.length === 0) {
    return <p className="text-xs text-zinc-400">Insufficient columns for chart</p>;
  }

  const labelFill = pieLabelFill(isDark);
  const lineStroke = pieLabelLineStroke(isDark);

  return (
    <div className="space-y-2">
      {spec.title && (
        <h4 className="text-xs font-semibold text-zinc-600 dark:text-zinc-400">
          {spec.title}
        </h4>
      )}
      <div className="h-72 w-full">
        <ResponsiveContainer width="100%" height="100%">
          <PieChart>
            <Pie
              data={chartData}
              cx="50%"
              cy="50%"
              innerRadius={50}
              outerRadius={90}
              paddingAngle={2}
              dataKey="value"
              nameKey="name"
              cursor="pointer"
              onClick={(_, index) => {
                const entry = chartData[index];
                if (!entry) return;
                useAppStore.getState().setDashboardFilter(labelField, entry.name);
              }}
              label={({
                x,
                y,
                name,
                percent,
                textAnchor,
              }: PieLabelRenderProps & { textAnchor: string }) => (
                <text
                  x={x as number}
                  y={y as number}
                  textAnchor={textAnchor}
                  dominantBaseline="central"
                  fontSize={11}
                  fill={labelFill}
                >
                  {`${name ?? ""} ${(((percent as number) ?? 0) * 100).toFixed(0)}%`}
                </text>
              )}
              labelLine={{ stroke: lineStroke, strokeWidth: 1 }}
            >
              {chartData.map((_, i) => (
                <Cell key={i} fill={PALETTE_PRIMARY[i % PALETTE_PRIMARY.length]} />
              ))}
            </Pie>
            <Tooltip
              formatter={(value: unknown) => {
                const v = Number(value ?? 0);
                return [
                  `${v.toLocaleString()} (${((v / total) * 100).toFixed(1)}%)`,
                ];
              }}
              contentStyle={tooltipStyle(isDark)}
            />
            {chartData.length <= CATEGORY_THRESHOLD && (
              <Legend
                wrapperStyle={{ fontSize: 11, color: isDark ? "#a1a1aa" : "#71717a" }}
                iconType="circle"
                iconSize={8}
              />
            )}
          </PieChart>
        </ResponsiveContainer>
      </div>
    </div>
  );
}
