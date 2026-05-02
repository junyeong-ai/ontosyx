"use client";

import { useMemo } from "react";
import { useTranslations } from "next-intl";
import type { QueryResult, WidgetSpec } from "@/types/api";
import {
  LineChart,
  Line,
  XAxis,
  YAxis,
  Tooltip,
  ResponsiveContainer,
  CartesianGrid,
  Area,
  AreaChart,
} from "recharts";
import { useIsDarkMode } from "@/hooks/use-dark-mode";
import {
  PALETTE_PRIMARY,
  CATEGORY_THRESHOLD,
  resolveLabelField,
  resolveValueField,
  toNameValuePairs,
  axisTickStyle,
  axisLineStroke,
  gridStroke,
  tooltipStyle,
} from "./chart-utils";

interface LineChartWidgetProps {
  spec: WidgetSpec;
  data: QueryResult;
}

export function LineChartWidget({ spec, data }: LineChartWidgetProps) {
  const t = useTranslations("widget.lineChart");
  const isDark = useIsDarkMode();
  const xField = resolveLabelField(spec, data);
  const yField = resolveValueField(spec, data);

  const chartData = useMemo(
    () => (xField && yField ? toNameValuePairs(data.rows, xField, yField) : []),
    [data.rows, xField, yField],
  );

  if (!xField || !yField || chartData.length === 0) {
    return <p className="text-xs text-muted-foreground">{t("insufficient")}</p>;
  }

  const isArea = spec.chart_type === "area";
  const ChartComponent = isArea ? AreaChart : LineChart;
  const strokeColor = PALETTE_PRIMARY[0];
  const rotated = chartData.length > CATEGORY_THRESHOLD;
  const tick = axisTickStyle(isDark);

  return (
    <div className="space-y-2">
      {spec.title && (
        <h4 className="text-xs font-semibold text-foreground dark:text-muted-foreground">
          {spec.title}
        </h4>
      )}
      <div className="h-64 w-full">
        <ResponsiveContainer width="100%" height="100%">
          <ChartComponent data={chartData} margin={{ top: 4, right: 8, left: 0, bottom: 4 }}>
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
            {isArea ? (
              <Area
                type="monotone"
                dataKey="value"
                stroke={strokeColor}
                fill={strokeColor}
                fillOpacity={0.15}
                strokeWidth={2}
                dot={{ r: 3, fill: strokeColor }}
                activeDot={{ r: 5 }}
              />
            ) : (
              <Line
                type="monotone"
                dataKey="value"
                stroke={strokeColor}
                strokeWidth={2}
                dot={{ r: 3, fill: strokeColor }}
                activeDot={{ r: 5 }}
              />
            )}
          </ChartComponent>
        </ResponsiveContainer>
      </div>
      <p className="text-2xs text-muted-foreground">{t("dataPointsCount", { count: chartData.length })}</p>
    </div>
  );
}
