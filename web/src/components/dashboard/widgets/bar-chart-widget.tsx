"use client";

import { useMemo } from "react";
import { useTranslations } from "next-intl";
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
import { useIsDarkMode } from "@/hooks/use-dark-mode";
import { Heading } from "@/components/ui/heading";
import {
  CATEGORY_THRESHOLD,
  MAX_BAR_SIZE,
  brandFill,
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
  const t = useTranslations("widget.barChart");
  const isDark = useIsDarkMode();
  const xField = resolveLabelField(spec, data);
  const yField = resolveValueField(spec, data);

  const chartData = useMemo(
    () => (xField && yField ? toNameValuePairs(data.rows, xField, yField) : []),
    [data.rows, xField, yField],
  );

  if (!xField || !yField || chartData.length === 0) {
    return <p className="text-xs text-foreground-muted">{t("insufficient")}</p>;
  }

  const rotated = chartData.length > CATEGORY_THRESHOLD;
  const tick = axisTickStyle(isDark);

  const ariaLabel = spec.title
    ? t("ariaLabelWithTitle", { title: spec.title, count: chartData.length })
    : t("ariaLabel", { count: chartData.length });

  return (
    <div className="space-y-2">
      {spec.title && (
        <Heading level={4} size={6}>
          {spec.title}
        </Heading>
      )}
      <div
        className="h-64 w-full overflow-hidden"
        role="img"
        aria-label={ariaLabel}
      >
        <ResponsiveContainer width="100%" height="100%">
          <BarChart
            data={chartData}
            margin={{ top: 4, right: 8, left: 0, bottom: 4 }}
            accessibilityLayer
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
            <YAxis tick={tick} axisLine={false} tickLine={false} width={50} />
            <Tooltip contentStyle={tooltipStyle(isDark)} />
            <Bar
              dataKey="value"
              fill={brandFill(isDark)}
              radius={[4, 4, 0, 0]}
              maxBarSize={MAX_BAR_SIZE}
              cursor="pointer"
              onClick={(data) => {
                if (!data?.payload) return;
                const label = data.payload.name;
                useAppStore.getState().setDashboardFilter(xField, label);
              }}
            />
          </BarChart>
        </ResponsiveContainer>
      </div>
      <p className="text-2xs text-foreground-muted">{t("itemsCount", { count: chartData.length })}</p>
    </div>
  );
}
