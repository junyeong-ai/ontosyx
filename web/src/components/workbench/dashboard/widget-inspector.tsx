"use client";

import { useEffect, useState } from "react";
import { toast } from "sonner";
import { updateWidget } from "@/lib/api";
import { WIDGET_TYPES } from "@/components/widgets/widget-types";
import type { DashboardWidget } from "@/types/api";

interface ThresholdConfig {
  warning?: number;
  critical?: number;
  direction?: "above" | "below";
}

export interface WidgetInspectorProps {
  widget: DashboardWidget;
  dashboardId: string;
  onUpdated: () => void;
}

export function WidgetInspector({ widget, dashboardId, onUpdated }: WidgetInspectorProps) {
  const [title, setTitle] = useState(widget.title);
  const [widgetType, setWidgetType] = useState(widget.widget_type);
  const [query, setQuery] = useState(widget.query ?? "");
  const [refreshSecs, setRefreshSecs] = useState(widget.refresh_interval_secs ?? 0);
  const [thresholds, setThresholds] = useState<ThresholdConfig>(widget.thresholds ?? {});
  const [isSaving, setIsSaving] = useState(false);

  // Reset when widget changes
  useEffect(() => {
    setTitle(widget.title);
    setWidgetType(widget.widget_type);
    setQuery(widget.query ?? "");
    setRefreshSecs(widget.refresh_interval_secs ?? 0);
    setThresholds(widget.thresholds ?? {});
  }, [widget.id, widget.title, widget.widget_type, widget.query, widget.refresh_interval_secs, widget.thresholds]);

  const origThresholds = widget.thresholds ?? {};
  const thresholdsChanged =
    thresholds.warning !== origThresholds.warning ||
    thresholds.critical !== origThresholds.critical ||
    (thresholds.direction ?? "above") !== (origThresholds.direction ?? "above");

  const hasChanges =
    title !== widget.title ||
    widgetType !== widget.widget_type ||
    query !== (widget.query ?? "") ||
    refreshSecs !== (widget.refresh_interval_secs ?? 0) ||
    thresholdsChanged;

  const handleSave = async () => {
    setIsSaving(true);
    try {
      await updateWidget(dashboardId, widget.id, {
        title: title !== widget.title ? title : undefined,
        widget_type: widgetType !== widget.widget_type ? widgetType : undefined,
        query: query !== (widget.query ?? "") ? query : undefined,
        refresh_interval_secs: refreshSecs !== (widget.refresh_interval_secs ?? 0) ? refreshSecs : undefined,
        thresholds: thresholdsChanged ? thresholds : undefined,
      });
      toast.success("Widget updated");
      onUpdated();
    } catch (err) {
      toast.error(`Failed to update: ${err instanceof Error ? err.message : String(err)}`);
    } finally {
      setIsSaving(false);
    }
  };

  return (
    <div className="space-y-4">
      <div>
        <label className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
          Title
        </label>
        <input
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          className="mt-0.5 w-full rounded-md border border-zinc-200 bg-white px-2 py-1.5 text-sm text-zinc-700 focus:border-emerald-400 focus:ring-1 focus:ring-emerald-400/50 focus:outline-none dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-300"
        />
      </div>
      <div>
        <label className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
          Chart Type
        </label>
        <select
          value={widgetType}
          onChange={(e) => setWidgetType(e.target.value)}
          className="mt-0.5 w-full rounded-md border border-zinc-200 bg-white px-2 py-1.5 text-sm text-zinc-700 focus:border-emerald-400 focus:ring-1 focus:ring-emerald-400/50 focus:outline-none dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-300"
        >
          {WIDGET_TYPES.map((t) => (
            <option key={t.value} value={t.value}>
              {t.label}
            </option>
          ))}
        </select>
      </div>
      <div>
        <label className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
          Cypher Query
        </label>
        <textarea
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          rows={4}
          className="mt-0.5 w-full rounded-md border border-zinc-200 bg-zinc-900 px-2 py-1.5 font-mono text-xs text-emerald-400 focus:border-emerald-400 focus:ring-1 focus:ring-emerald-400/50 focus:outline-none dark:border-zinc-700"
          placeholder="MATCH (n) RETURN n.name, count(*) LIMIT 10"
        />
      </div>
      <div>
        <label className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
          Auto-Refresh (seconds)
        </label>
        <input
          type="number"
          min={0}
          value={refreshSecs}
          onChange={(e) => setRefreshSecs(parseInt(e.target.value) || 0)}
          className="mt-0.5 w-full rounded-md border border-zinc-200 bg-white px-2 py-1.5 text-sm text-zinc-700 focus:border-emerald-400 focus:ring-1 focus:ring-emerald-400/50 focus:outline-none dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-300"
          placeholder="0 = disabled"
        />
        <p className="mt-0.5 text-[10px] text-zinc-400">0 = no auto-refresh</p>
      </div>
      <div>
        <label className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
          KPI Thresholds
        </label>
        <div className="mt-0.5 grid grid-cols-2 gap-2">
          <div>
            <label className="text-[9px] text-zinc-400">Warning</label>
            <input
              type="number"
              value={thresholds.warning ?? ""}
              onChange={(e) =>
                setThresholds((prev) => ({
                  ...prev,
                  warning: e.target.value ? Number(e.target.value) : undefined,
                }))
              }
              className="w-full rounded-md border border-zinc-200 bg-white px-2 py-1 text-sm text-zinc-700 focus:border-emerald-400 focus:ring-1 focus:ring-emerald-400/50 focus:outline-none dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-300"
              placeholder="e.g. 80"
            />
          </div>
          <div>
            <label className="text-[9px] text-zinc-400">Critical</label>
            <input
              type="number"
              value={thresholds.critical ?? ""}
              onChange={(e) =>
                setThresholds((prev) => ({
                  ...prev,
                  critical: e.target.value ? Number(e.target.value) : undefined,
                }))
              }
              className="w-full rounded-md border border-zinc-200 bg-white px-2 py-1 text-sm text-zinc-700 focus:border-emerald-400 focus:ring-1 focus:ring-emerald-400/50 focus:outline-none dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-300"
              placeholder="e.g. 95"
            />
          </div>
        </div>
        <select
          value={thresholds.direction ?? "above"}
          onChange={(e) =>
            setThresholds((prev) => ({
              ...prev,
              direction: e.target.value as "above" | "below",
            }))
          }
          className="mt-1 w-full rounded-md border border-zinc-200 bg-white px-2 py-1 text-xs text-zinc-700 focus:border-emerald-400 focus:ring-1 focus:ring-emerald-400/50 focus:outline-none dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-300"
        >
          <option value="above">Alert when above threshold</option>
          <option value="below">Alert when below threshold</option>
        </select>
      </div>
      {hasChanges && (
        <button
          onClick={handleSave}
          disabled={isSaving}
          className="w-full rounded-md bg-emerald-600 px-3 py-1.5 text-xs font-medium text-white transition-colors hover:bg-emerald-700 disabled:opacity-50"
        >
          {isSaving ? "Saving..." : "Save Changes"}
        </button>
      )}
    </div>
  );
}
