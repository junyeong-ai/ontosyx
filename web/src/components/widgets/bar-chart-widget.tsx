"use client";

import { useMemo } from "react";
import type { QueryResult, WidgetSpec } from "@/types/api";
import {
  BarChart,
  Bar,
  XAxis,
  YAxis,
  Tooltip,
  ResponsiveContainer,
  CartesianGrid,
} from "recharts";
import { useAppStore } from "@/lib/store";
import { useIsDarkMode } from "@/lib/use-dark-mode";
import {
  CATEGORY_THRESHOLD,
  MAX_BAR_SIZE,
  resolveLabelField,
  resolveValueField,
  toNameValuePairs,
  axisTickStyle,
  axisLineStroke,
  gridStroke,
  tooltipStyle,
} from "./chart-utils";

interface BarChartWidgetProps {
  spec: WidgetSpec;
  data: QueryResult;
}

export function BarChartWidget({ spec, data }: BarChartWidgetProps) {
  const isDark = useIsDarkMode();
  const xField = resolveLabelField(spec, data);
  const yField = resolveValueField(spec, data);

  const chartData = useMemo(
    () => (xField && yField ? toNameValuePairs(data.rows, xField, yField) : []),
    [data.rows, xField, yField],
  );

  if (!xField || !yField || chartData.length === 0) {
    return <p className="text-xs text-zinc-400">Insufficient columns for chart</p>;
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
      <div className="h-64 w-full overflow-hidden">
        <ResponsiveContainer width="100%" height="100%">
          <BarChart data={chartData} margin={{ top: 4, right: 8, left: 0, bottom: 4 }}>
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
            <YAxis tick={tick} axisLine={false} tickLine={false} width={50} />
            <Tooltip contentStyle={tooltipStyle(isDark)} />
            <Bar
              dataKey="value"
              fill="#10b981"
              radius={[4, 4, 0, 0]}
              maxBarSize={MAX_BAR_SIZE}
              cursor="pointer"
              onClick={(data) => {
                if (!data || !data.payload) return;
                const label = data.payload.name;
                useAppStore.getState().setDashboardFilter(xField, label);
              }}
            />
          </BarChart>
        </ResponsiveContainer>
      </div>
      <p className="text-[10px] text-zinc-400">{chartData.length} items</p>
    </div>
  );
}
