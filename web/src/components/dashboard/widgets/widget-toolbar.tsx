"use client";

import { useState } from "react";
import { useTranslations } from "next-intl";
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

const WIDGET_OPTIONS: readonly { type: string; icon: IconSvgElement; labelKey: string }[] = [
  { type: "table", icon: Table01Icon, labelKey: "table" },
  { type: "graph", icon: Share01Icon, labelKey: "graph" },
  { type: "bar_chart", icon: ChartColumnIcon, labelKey: "barChart" },
  { type: "combo_chart", icon: Layers01Icon, labelKey: "comboChart" },
  { type: "pie_chart", icon: PieChartIcon, labelKey: "pieChart" },
  { type: "line_chart", icon: ChartLineData01Icon, labelKey: "lineChart" },
  { type: "stat_card", icon: HashtagIcon, labelKey: "statCard" },
];

export function WidgetToolbar({ spec, data }: WidgetWithToolbarProps) {
  const t = useTranslations("widget.toolbar");
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
        <div className="flex items-center gap-0.5 rounded-lg bg-surface-inset p-0.5/80 w-fit">
          {available.map(({ type, icon, labelKey }) => {
            const label = t(labelKey);
            return (
              <Tooltip key={type} content={label}>
                <button
                  onClick={() => setActiveType(type)}
                  className={cn(
                    "flex items-center gap-1 rounded-md px-2 py-1 text-xs transition-all",
                    activeType === type
                      ? "bg-surface-base text-foreground-strong shadow-sm-strong"
                      : "text-foreground-muted hover:text-foreground dark:text-muted-foreground dark:hover:text-foreground-strong",
                  )}
                >
                  <HugeiconsIcon icon={icon} className="h-3.5 w-3.5" size="100%" />
                  <span className="hidden sm:inline">{label}</span>
                </button>
              </Tooltip>
            );
          })}
        </div>
      )}

      {/* Widget */}
      <WidgetRenderer spec={currentSpec} data={data} />
    </div>
  );
}
