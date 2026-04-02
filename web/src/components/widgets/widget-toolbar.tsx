"use client";

import { useState } from "react";
import { cn } from "@/lib/cn";
import type { QueryResult, WidgetSpec } from "@/types/api";
import { WidgetRenderer, viableTypes } from "./widget-renderer";
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

  // Use shared viableTypes logic for consistent filtering
  const viable = viableTypes(data);

  const available = WIDGET_OPTIONS.filter(({ type }) => viable.has(type));

  const currentSpec: WidgetSpec = { ...spec, widget_type: activeType };

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
