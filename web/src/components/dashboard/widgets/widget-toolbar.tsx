"use client";

import { useState } from "react";
import { useTranslations } from "next-intl";
import { cn } from "@/lib/cn";
import { CodeEditor } from "@/components/ui/code-editor";
import type { QueryResult, WidgetSpec } from "@/types/api";
import { WidgetRenderer, viableTypes } from "./widget-renderer";
import { Tooltip } from "@/components/ui/tooltip";
import type { LucideIcon } from "lucide-react";
import { DynamicIcon } from "@/components/ui/dynamic-icon";
import {
  BarChart,
  Braces,
  Hash,
  Layers,
  LineChart,
  PieChart,
  Share2,
  Table,
} from "lucide-react";
interface WidgetWithToolbarProps {
  /** Initial widget spec (from LLM hint or auto) */
  spec: WidgetSpec;
  data: QueryResult;
}

/// `RAW_VIEW_TYPE` is a toolbar-only sentinel that bypasses the
/// `WidgetRenderer` — selecting it shows the raw QueryResult as
/// syntax-highlighted JSON. Distinct from the renderer's widget
/// types (`table`, `graph`, ...) because it isn't a data
/// visualisation, it's an inspection affordance for advanced
/// operators / debugging.
const RAW_VIEW_TYPE = "__raw__";

const WIDGET_OPTIONS: readonly { type: string; icon: LucideIcon; labelKey: string }[] = [
  { type: "table", icon: Table, labelKey: "table" },
  { type: "graph", icon: Share2, labelKey: "graph" },
  { type: "bar_chart", icon: BarChart, labelKey: "barChart" },
  { type: "combo_chart", icon: Layers, labelKey: "comboChart" },
  { type: "pie_chart", icon: PieChart, labelKey: "pieChart" },
  { type: "line_chart", icon: LineChart, labelKey: "lineChart" },
  { type: "stat_card", icon: Hash, labelKey: "statCard" },
  { type: RAW_VIEW_TYPE, icon: Braces, labelKey: "raw" },
];

export function WidgetToolbar({ spec, data }: WidgetWithToolbarProps) {
  const t = useTranslations("widget.toolbar");
  const initialType = spec.widget_type ?? "auto";
  const [activeType, setActiveType] = useState<string>(initialType);

  // Don't show toolbar for trivial data
  if (!data.rows.length || !data.columns.length) return null;

  // Use shared viableTypes logic for consistent filtering. The
  // raw JSON view is always viable — it's a debugging affordance
  // independent of the visualisation eligibility heuristics.
  const viable = viableTypes(data);
  viable.add(RAW_VIEW_TYPE);

  const available = WIDGET_OPTIONS.filter(({ type }) => viable.has(type));

  const isRawView = activeType === RAW_VIEW_TYPE;
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
                  type="button"
                  onClick={() => setActiveType(type)}
                  className={cn(
                    "flex items-center gap-1 rounded-md px-2 py-1 text-xs transition-all duration-[var(--duration-base)] ease-[var(--ease-out)]",
                    activeType === type
                      ? "bg-surface-base text-foreground-strong shadow-1-strong"
                      : "text-foreground-muted hover:text-foreground-strong",
                  )}
                >
                  <DynamicIcon as={icon} className="h-3.5 w-3.5" />
                  <span className="hidden sm:inline">{label}</span>
                </button>
              </Tooltip>
            );
          })}
        </div>
      )}

      {/* Widget */}
      {isRawView ? (
        <CodeEditor
          value={JSON.stringify(data, null, 2)}
          language="json"
          readOnly
          height="360px"
          ariaLabel={t("rawAriaLabel")}
        />
      ) : (
        <WidgetRenderer spec={currentSpec} data={data} />
      )}
    </div>
  );
}
