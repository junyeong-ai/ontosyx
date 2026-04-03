/* eslint-disable react-hooks/rules-of-hooks */
"use client";

import { useState } from "react";
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

export interface WidgetRendererProps {
  spec: WidgetSpec;
  data: QueryResult;
}

function JsonWidget({ data }: { data: QueryResult }) {
  return (
    <pre
      className={cn(
        "max-h-64 overflow-auto rounded-lg p-3 text-xs",
        "bg-zinc-950 text-emerald-400",
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
function autoDetectWidgetType(data: QueryResult, _spec: WidgetSpec): string {
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

  // 2 numeric columns with many rows → scatter
  if (numericCount === 2 && numRows >= 5 && numCols === 2) {
    return "scatter";
  }

  // Single numeric column (no string label) → histogram
  if (numCols === 1 && numRows >= 5 && numericCount === 1) {
    return "histogram";
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
  { value: "table", label: "Table", icon: "≡" },
  { value: "graph", label: "Graph", icon: "◉" },
  { value: "bar_chart", label: "Bar", icon: "▐" },
  { value: "pie_chart", label: "Pie", icon: "◕" },
  { value: "line_chart", label: "Line", icon: "⌇" },
  { value: "combo_chart", label: "Combo", icon: "⊞" },
  { value: "scatter", label: "Scatter", icon: "∴" },
  { value: "stat_card", label: "Stat", icon: "#" },
  { value: "heatmap", label: "Heat", icon: "▦" },
  { value: "treemap", label: "Tree", icon: "▣" },
  { value: "funnel", label: "Funnel", icon: "▽" },
  { value: "timeline", label: "Time", icon: "│" },
] as const;

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

export function WidgetRenderer({ spec, data }: WidgetRendererProps) {
  let defaultType = resolveWidgetType(spec);

  // "none" from LLM means text summary is enough
  if (defaultType === "none") return null;

  // Auto-detect if no explicit type
  if (defaultType === "auto") {
    defaultType = autoDetectWidgetType(data, spec);
  }

  // Normalize "chart" to specific chart type
  if (defaultType === "chart") {
    const ct = spec.chart_type;
    defaultType = ct === "pie" ? "pie_chart" : ct === "line" ? "line_chart" : "bar_chart";
  }

  const [activeType, setActiveType] = useState(defaultType);
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
      case "graph": return <GraphWidget spec={s} data={data} />;
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
        <div className="flex gap-0.5 rounded-md bg-zinc-100 p-0.5 dark:bg-zinc-800">
          {SWITCHABLE_TYPES.filter(({ value }) => viable.has(value)).map(({ value, label, icon }) => (
            <button
              key={value}
              onClick={() => setActiveType(value)}
              className={cn(
                "rounded px-2 py-1 text-[10px] font-medium transition-colors",
                activeType === value
                  ? "bg-white text-zinc-900 shadow-sm dark:bg-zinc-700 dark:text-zinc-100"
                  : "text-zinc-500 hover:text-zinc-700 dark:text-zinc-400 dark:hover:text-zinc-200",
              )}
              title={label}
            >
              {icon} {label}
            </button>
          ))}
        </div>
      )}
      {renderWidget(activeType)}
    </div>
  );
}
