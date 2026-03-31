"use client";

import { useState } from "react";
import { cn } from "@/lib/cn";
import type { QueryResult, WidgetSpec } from "@/types/api";
import { WidgetRenderer } from "./widget-renderer";
import { CATEGORY_THRESHOLD } from "./chart-utils";
import { Tooltip } from "@/components/ui/tooltip";
import { HugeiconsIcon, type IconSvgElement } from "@hugeicons/react";
import {
  Table01Icon,
  ChartColumnIcon,
  PieChartIcon,
  ChartLineData01Icon,
  HashtagIcon,
  Layers01Icon,
  Share01Icon,
} from "@hugeicons/core-free-icons";

interface WidgetWithToolbarProps {
  /** Initial widget spec (from LLM hint or auto) */
  spec: WidgetSpec;
  data: QueryResult;
}

/** Column names indicating graph-like edge data (mirrored from widget-renderer) */
const GRAPH_SOURCE_COLS = new Set(["source", "source_id", "from"]);
const GRAPH_TARGET_COLS = new Set(["target", "target_id", "to"]);

const WIDGET_OPTIONS: readonly { type: string; icon: IconSvgElement; label: string }[] = [
  { type: "table", icon: Table01Icon, label: "Table" },
  { type: "graph", icon: Share01Icon, label: "Graph" },
  { type: "bar_chart", icon: ChartColumnIcon, label: "Bar Chart" },
  { type: "combo_chart", icon: Layers01Icon, label: "Combo Chart" },
  { type: "pie_chart", icon: PieChartIcon, label: "Pie Chart" },
  { type: "line_chart", icon: ChartLineData01Icon, label: "Line Chart" },
  { type: "stat_card", icon: HashtagIcon, label: "Stat Card" },
];

export function WidgetWithToolbar({ spec, data }: WidgetWithToolbarProps) {
  const initialType = spec.widget_type ?? "auto";
  const [activeType, setActiveType] = useState<string>(initialType);

  // Don't show toolbar for trivial data
  if (!data.rows.length || !data.columns.length) return null;

  const numCols = data.columns.length;
  const numRows = data.rows.length;
  const numericColCount = data.columns.filter(
    (col) => typeof data.rows[0][col] === "number",
  ).length;

  // Detect graph-compatible data
  const lowerCols = data.columns.map((c) => c.toLowerCase());
  const hasGraphCols =
    lowerCols.some((c) => GRAPH_SOURCE_COLS.has(c)) &&
    lowerCols.some((c) => GRAPH_TARGET_COLS.has(c));

  const available = WIDGET_OPTIONS.filter(({ type }) => {
    if (type === "table") return true;
    if (type === "graph") return hasGraphCols || numRows >= 2;
    if (type === "stat_card") return numRows <= 3 && numCols <= 3;
    if (type === "combo_chart")
      return numCols >= 3 && numRows >= 2 && numericColCount >= 2;
    if (type === "pie_chart")
      return numCols >= 2 && numRows >= 2 && numRows <= CATEGORY_THRESHOLD;
    // bar_chart, line_chart
    return numCols >= 2 && numRows >= 2;
  });

  const currentSpec: WidgetSpec = { ...spec, widget: activeType };

  return (
    <div className="space-y-1.5">
      {/* Toolbar */}
      {available.length > 1 && (
        <div className="flex items-center gap-0.5 rounded-lg bg-zinc-100 p-0.5 dark:bg-zinc-800/80 w-fit">
          {available.map(({ type, icon, label }) => (
            <Tooltip key={type} content={label}>
              <button
                onClick={() => setActiveType(type)}
                className={cn(
                  "flex items-center gap-1 rounded-md px-2 py-1 text-xs transition-all",
                  activeType === type
                    ? "bg-white text-zinc-900 shadow-sm dark:bg-zinc-700 dark:text-zinc-100"
                    : "text-zinc-500 hover:text-zinc-700 dark:text-zinc-400 dark:hover:text-zinc-200",
                )}
              >
                <HugeiconsIcon icon={icon} className="h-3.5 w-3.5" size="100%" />
                <span className="hidden sm:inline">{label}</span>
              </button>
            </Tooltip>
          ))}
        </div>
      )}

      {/* Widget */}
      <WidgetRenderer spec={currentSpec} data={data} />
    </div>
  );
}
