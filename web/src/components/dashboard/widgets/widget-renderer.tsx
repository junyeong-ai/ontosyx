"use client";

import { useState } from "react";
import { useTranslations } from "next-intl";
import type { QueryResult, WidgetSpec } from "@/types/api";
import { TableWidget } from "./table-widget";
import { BarChartWidget } from "./bar-chart-widget";
import { PieChartWidget } from "./pie-chart-widget";
import { LineChartWidget } from "./line-chart-widget";
import { StatCardWidget } from "./stat-card-widget";
import { ComboChartWidget } from "./combo-chart-widget";
import { GraphWidget } from "./graph-widget";
import { ScatterChartWidget } from "./scatter-chart-widget";
import { HistogramWidget } from "./histogram-widget";
import { HeatmapWidget } from "./heatmap-widget";
import { TimelineWidget } from "./timeline-widget";
import { TreemapWidget } from "./treemap-widget";
import { FunnelWidget } from "./funnel-widget";
import { CATEGORY_THRESHOLD } from "./chart-utils";
import { cn } from "@/lib/cn";
import { WidgetErrorBoundary } from "./widget-error-boundary";

export interface WidgetRendererProps {
  spec: WidgetSpec;
  data: QueryResult;
  /**
   * Optional dashboard id — forwarded to type-filter-aware widgets
   * (currently `graph`) so they share the hidden-types set across
   * every widget mounted in the same dashboard. Non-dashboard mount
   * sites (query panel, chat execution detail) omit it and the
   * widgets fall back to local-state filtering.
   */
  dashboardId?: string | null;
}

function JsonWidget({ data }: { data: QueryResult }) {
  return (
    <pre
      className={cn(
        "max-h-64 overflow-auto rounded-lg p-3 text-xs",
        "bg-surface-base text-brand-foreground",
      )}
    >
      {JSON.stringify(data.rows, null, 2)}
    </pre>
  );
}

/**
 * Resolve the widget type from spec.
 * Resolves the canonical widget type from spec.widget_type.
 */
function resolveWidgetType(spec: WidgetSpec): string {
  return spec.widget_type ?? "auto";
}

/** Column names that indicate graph-like edge data */
const GRAPH_SOURCE_COLS = new Set(["source", "source_id", "from"]);
const GRAPH_TARGET_COLS = new Set(["target", "target_id", "to"]);
const GRAPH_REL_COLS = new Set(["relationship", "rel_type", "edge_type"]);

/**
 * Check whether query result data looks like a graph (edges or path data).
 */
function looksLikeGraph(data: QueryResult): boolean {
  if (data.columns.length < 2 || data.rows.length < 2) return false;
  const lower = data.columns.map((c) => c.toLowerCase());
  const hasSource = lower.some((c) => GRAPH_SOURCE_COLS.has(c));
  const hasTarget = lower.some((c) => GRAPH_TARGET_COLS.has(c));
  const hasRel = lower.some((c) => GRAPH_REL_COLS.has(c));
  return (hasSource && hasTarget) || (hasSource && hasRel) || (hasTarget && hasRel);
}

/**
 * Auto-detect the best widget type from data shape.
 * Used as fallback when LLM hint is unavailable.
 */
function autoDetectWidgetType(data: QueryResult): string {
  const { columns, rows } = data;
  if (!columns.length || !rows.length) return "table";

  // Graph detection — edge-like columns or PathFind operation
  if (looksLikeGraph(data)) return "graph";

  const numCols = columns.length;
  const numRows = rows.length;
  const firstRow = rows[0];

  // Single row, 1-2 numeric columns → stat card
  if (numRows === 1 && numCols <= 2) {
    const allNumeric = columns.every(
      (col) => typeof firstRow[col] === "number",
    );
    if (allNumeric) return "stat_card";
  }

  // Count numeric columns
  const numericCount = columns.filter(
    (col) => typeof firstRow[col] === "number",
  ).length;

  // Exactly 2 numeric columns with sufficient rows → scatter
  if (numericCount >= 2 && numRows >= 5 && numCols <= 3) {
    return "scatter";
  }

  // Single numeric column (pure distribution data) → histogram
  if (numericCount === 1 && numCols <= 2 && numRows >= 5) {
    const stringCount = numCols - numericCount;
    if (stringCount === 0) return "histogram";
  }

  // 3+ columns: label + 2+ numeric → combo chart
  if (numCols >= 3 && numRows >= 2 && numericCount >= 2) {
    return "combo_chart";
  }

  // 2 columns: label + number → chart
  if (numCols === 2 && numRows >= 2) {
    const [col1, col2] = columns;
    const isLabelValue =
      typeof firstRow[col1] === "string" && typeof firstRow[col2] === "number";
    const isValueLabel =
      typeof firstRow[col1] === "number" && typeof firstRow[col2] === "string";

    if (isLabelValue || isValueLabel) {
      if (numRows <= CATEGORY_THRESHOLD) return "pie_chart";
      return "bar_chart";
    }
  }

  return "table";
}

/** Chart types available for user switching. */
const SWITCHABLE_TYPES = [
  { value: "table", labelKey: "typeTable", icon: "≡" },
  { value: "graph", labelKey: "typeGraph", icon: "◉" },
  { value: "bar_chart", labelKey: "typeBar", icon: "▐" },
  { value: "pie_chart", labelKey: "typePie", icon: "◕" },
  { value: "line_chart", labelKey: "typeLine", icon: "⌇" },
  { value: "combo_chart", labelKey: "typeCombo", icon: "⊞" },
  { value: "scatter", labelKey: "typeScatter", icon: "∴" },
  { value: "histogram", labelKey: "typeHistogram", icon: "▌" },
  { value: "stat_card", labelKey: "typeStat", icon: "#" },
  { value: "heatmap", labelKey: "typeHeat", icon: "▦" },
  { value: "treemap", labelKey: "typeTree", icon: "▣" },
  { value: "funnel", labelKey: "typeFunnel", icon: "▽" },
  { value: "timeline", labelKey: "typeTime", icon: "│" },
] as const;

type SwitchableTypeKey = (typeof SWITCHABLE_TYPES)[number]["labelKey"];

/** Determine which chart types are viable for given data shape. */
export function viableTypes(data: QueryResult): Set<string> {
  const viable = new Set<string>(["table"]); // table always works
  const { columns, rows } = data;
  if (!columns.length || !rows.length) return viable;

  const firstRow = rows[0];
  const numericCols = columns.filter((c) => typeof firstRow[c] === "number");
  const stringCols = columns.filter((c) => typeof firstRow[c] === "string");

  // bar/pie/line: need at least 1 string + 1 numeric column
  if (stringCols.length >= 1 && numericCols.length >= 1) {
    viable.add("bar_chart");
    viable.add("line_chart");
    if (rows.length <= CATEGORY_THRESHOLD) viable.add("pie_chart");
  }
  // combo: 1 string + 2+ numeric
  if (stringCols.length >= 1 && numericCols.length >= 2) {
    viable.add("combo_chart");
  }
  // scatter: 2+ numeric columns, enough data points to be meaningful
  if (numericCols.length >= 2 && rows.length >= 5) {
    viable.add("scatter");
  }
  // stat card: 1 row, 1-2 numeric columns
  if (rows.length === 1 && numericCols.length >= 1 && columns.length <= 2) {
    viable.add("stat_card");
  }
  // graph: edge-like columns
  if (looksLikeGraph(data)) {
    viable.add("graph");
  }
  // histogram: 1 numeric column with enough rows
  if (numericCols.length >= 1 && rows.length >= 5) {
    viable.add("histogram");
  }
  // heatmap: 3+ columns with at least 1 numeric
  if (columns.length >= 3 && numericCols.length >= 1 && rows.length >= 2) {
    viable.add("heatmap");
  }
  // treemap: 1 string + 1 numeric
  if (stringCols.length >= 1 && numericCols.length >= 1) {
    viable.add("treemap");
  }
  // funnel: 1 string + 1 numeric, small row count
  if (stringCols.length >= 1 && numericCols.length >= 1 && rows.length <= 20) {
    viable.add("funnel");
  }
  // timeline: date-like column detected
  if (rows.length >= 2) {
    const dateCols = columns.filter((c) => {
      const l = c.toLowerCase();
      return ["date", "timestamp", "time", "created", "updated"].some((p) => l.includes(p));
    });
    if (dateCols.length >= 1) viable.add("timeline");
  }

  return viable;
}

function resolveInitialType(spec: WidgetSpec, data: QueryResult): string {
  const raw = resolveWidgetType(spec);
  if (raw === "none") return "none";
  if (raw === "auto") return autoDetectWidgetType(data);
  if (raw === "chart") {
    const ct = spec.chart_type;
    return ct === "pie" ? "pie_chart" : ct === "line" ? "line_chart" : "bar_chart";
  }
  return raw;
}

export function WidgetRenderer({ spec, data, dashboardId }: WidgetRendererProps) {
  const t = useTranslations("widget.renderer");
  const initialType = resolveInitialType(spec, data);
  const [activeType, setActiveType] = useState(initialType);

  if (initialType === "none") return null;

  const viable = viableTypes(data);

  // Only show switcher when multiple chart types are viable and data has rows
  const showSwitcher = viable.size > 2 && data.rows.length > 0;

  const renderWidget = (type: string) => {
    const s = { ...spec, widget_type: type };
    switch (type) {
      case "table": return <TableWidget spec={s} data={data} />;
      case "bar_chart": return <BarChartWidget spec={s} data={data} />;
      case "pie_chart": return <PieChartWidget spec={s} data={data} />;
      case "line_chart": return <LineChartWidget spec={s} data={data} />;
      case "combo_chart": return <ComboChartWidget spec={s} data={data} />;
      case "stat_card": case "text": return <StatCardWidget spec={s} data={data} />;
      case "scatter": return <ScatterChartWidget spec={s} data={data} />;
      case "histogram": return <HistogramWidget spec={s} data={data} />;
      case "graph": return <GraphWidget spec={s} data={data} dashboardId={dashboardId} />;
      case "heatmap": return <HeatmapWidget spec={s} data={data} />;
      case "timeline": return <TimelineWidget spec={s} data={data} />;
      case "treemap": return <TreemapWidget spec={s} data={data} />;
      case "funnel": return <FunnelWidget spec={s} data={data} />;
      case "code": case "json": return <JsonWidget data={data} />;
      default: return <TableWidget spec={s} data={data} />;
    }
  };

  return (
    <div className="space-y-1">
      {showSwitcher && (
        <div className="flex gap-0.5 rounded-md bg-surface-inset p-0.5">
          {SWITCHABLE_TYPES.filter(({ value }) => viable.has(value)).map(({ value, labelKey, icon }) => {
            const label = t(labelKey satisfies SwitchableTypeKey);
            return (
              <button
                key={value}
                onClick={() => setActiveType(value)}
                className={cn(
                  "rounded px-2 py-1 text-2xs font-medium transition-colors",
                  activeType === value
                    ? "bg-surface-base text-foreground-strong shadow-sm-strong"
                    : "text-foreground-muted hover:text-foreground dark:text-muted-foreground dark:hover:text-foreground-strong",
                )}
                title={label}
              >
                {icon} {label}
              </button>
            );
          })}
        </div>
      )}
      <WidgetErrorBoundary widgetType={activeType}>
        {renderWidget(activeType)}
      </WidgetErrorBoundary>
    </div>
  );
}
