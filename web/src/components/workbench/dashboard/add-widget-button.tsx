"use client";

import { useState } from "react";
import { HugeiconsIcon } from "@hugeicons/react";
import { PlusSignIcon } from "@hugeicons/core-free-icons";
import { toast } from "sonner";
import { addWidget } from "@/lib/api";
import { WIDGET_TYPES } from "@/components/widgets/widget-types";
import type { DashboardWidget } from "@/types/api";

// ---------------------------------------------------------------------------
// Query templates for quick-start
// ---------------------------------------------------------------------------
const TEMPLATES = [
  {
    label: "Count by type",
    query:
      "MATCH (n) RETURN labels(n)[0] AS type, count(n) AS count ORDER BY count DESC",
  },
  {
    label: "Top 10 nodes",
    query:
      "MATCH (n) RETURN n.name AS name, labels(n)[0] AS type LIMIT 10",
  },
  {
    label: "Relationship distribution",
    query:
      "MATCH ()-[r]->() RETURN type(r) AS rel_type, count(*) AS count ORDER BY count DESC",
  },
];

// ---------------------------------------------------------------------------
// Smart placement — find the first open row below all existing widgets
// ---------------------------------------------------------------------------
function findNextPosition(
  widgets: DashboardWidget[],
): { x: number; y: number; w: number; h: number } {
  if (!widgets || widgets.length === 0) return { x: 0, y: 0, w: 6, h: 4 };
  const maxY = Math.max(
    ...widgets.map((w) => {
      const pos = w.position as
        | { x?: number; y?: number; w?: number; h?: number }
        | undefined;
      return (pos?.y ?? 0) + (pos?.h ?? 4);
    }),
  );
  return { x: 0, y: maxY, w: 6, h: 4 };
}

// ---------------------------------------------------------------------------
// Public interface
// ---------------------------------------------------------------------------
export interface AddWidgetButtonProps {
  dashboardId: string;
  existingWidgets: DashboardWidget[];
  onAdded: (w: DashboardWidget) => void;
}

export function AddWidgetButton({
  dashboardId,
  existingWidgets,
  onAdded,
}: AddWidgetButtonProps) {
  const [open, setOpen] = useState(false);
  const [title, setTitle] = useState("");
  const [query, setQuery] = useState("");
  const [widgetType, setWidgetType] = useState("table");
  const [isSaving, setIsSaving] = useState(false);

  const resetForm = () => {
    setTitle("");
    setQuery("");
    setWidgetType("table");
  };

  const handleSave = async () => {
    if (!title.trim() || !query.trim()) return;
    setIsSaving(true);
    try {
      const position = findNextPosition(existingWidgets);
      const widget = await addWidget(dashboardId, {
        title: title.trim(),
        widget_type: widgetType,
        query: query.trim(),
        widget_spec: {},
        position,
      });
      onAdded(widget);
      resetForm();
      setOpen(false);
      toast.success("Widget added");
    } catch {
      toast.error("Failed to add widget");
    } finally {
      setIsSaving(false);
    }
  };

  return (
    <>
      {/* Trigger button */}
      <button
        onClick={() => setOpen(true)}
        className="flex w-full items-center justify-center gap-2 rounded-lg border-2 border-dashed border-zinc-300 py-6 text-xs text-zinc-500 transition-colors hover:border-emerald-400 hover:text-emerald-600 dark:border-zinc-700 dark:hover:border-emerald-600"
      >
        <HugeiconsIcon icon={PlusSignIcon} className="h-4 w-4" size="100%" />
        Add Widget
      </button>

      {/* Modal overlay */}
      {open && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-sm">
          <div
            className="w-full max-w-lg rounded-xl border border-zinc-200 bg-white p-6 shadow-xl dark:border-zinc-700 dark:bg-zinc-900"
            onKeyDown={(e) => {
              if (e.key === "Escape") {
                setOpen(false);
                resetForm();
              }
            }}
          >
            {/* Header */}
            <h3 className="text-sm font-semibold text-zinc-900 dark:text-zinc-100">
              Add Widget
            </h3>
            <p className="mt-1 text-xs text-zinc-500">
              Create a new dashboard widget from a Cypher query.
            </p>

            <div className="mt-4 space-y-4">
              {/* Title */}
              <div>
                <label className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
                  Title
                </label>
                <input
                  value={title}
                  onChange={(e) => setTitle(e.target.value)}
                  placeholder="Widget title"
                  autoFocus
                  className="mt-1 w-full rounded-md border border-zinc-200 bg-white px-3 py-1.5 text-sm text-zinc-700 focus:border-emerald-400 focus:ring-1 focus:ring-emerald-400/50 focus:outline-none dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-300"
                />
              </div>

              {/* Widget type */}
              <div>
                <label className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
                  Type
                </label>
                <select
                  value={widgetType}
                  onChange={(e) => setWidgetType(e.target.value)}
                  className="mt-1 w-full rounded-md border border-zinc-200 bg-white px-3 py-1.5 text-sm text-zinc-700 focus:border-emerald-400 focus:ring-1 focus:ring-emerald-400/50 focus:outline-none dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-300"
                >
                  {WIDGET_TYPES.map((t) => (
                    <option key={t.value} value={t.value}>
                      {t.label}
                    </option>
                  ))}
                </select>
              </div>

              {/* Templates */}
              <div>
                <label className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
                  Templates
                </label>
                <div className="mt-1 flex flex-wrap gap-1.5">
                  {TEMPLATES.map((tpl) => (
                    <button
                      key={tpl.label}
                      type="button"
                      onClick={() => setQuery(tpl.query)}
                      className="rounded-full border border-zinc-200 px-2.5 py-1 text-[11px] text-zinc-600 transition-colors hover:border-emerald-400 hover:bg-emerald-50 hover:text-emerald-700 dark:border-zinc-700 dark:text-zinc-400 dark:hover:border-emerald-600 dark:hover:bg-emerald-950/30 dark:hover:text-emerald-400"
                    >
                      {tpl.label}
                    </button>
                  ))}
                </div>
              </div>

              {/* Cypher query */}
              <div>
                <label className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
                  Cypher Query
                </label>
                <textarea
                  value={query}
                  onChange={(e) => setQuery(e.target.value)}
                  placeholder="MATCH (n) RETURN labels(n)[0] AS label, count(n) AS cnt"
                  rows={6}
                  className="mt-1 w-full rounded-md border border-zinc-200 bg-white px-3 py-2 font-mono text-xs text-zinc-700 focus:border-emerald-400 focus:ring-1 focus:ring-emerald-400/50 focus:outline-none dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-300"
                />
              </div>
            </div>

            {/* Footer buttons */}
            <div className="mt-5 flex justify-end gap-2">
              <button
                onClick={() => {
                  setOpen(false);
                  resetForm();
                }}
                className="rounded-lg px-4 py-2 text-sm font-medium text-zinc-600 transition-colors hover:bg-zinc-100 dark:text-zinc-400 dark:hover:bg-zinc-800"
              >
                Cancel
              </button>
              <button
                onClick={handleSave}
                disabled={!title.trim() || !query.trim() || isSaving}
                className="rounded-lg bg-emerald-600 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-emerald-700 disabled:opacity-50"
              >
                {isSaving ? "Adding..." : "Add Widget"}
              </button>
            </div>
          </div>
        </div>
      )}
    </>
  );
}
