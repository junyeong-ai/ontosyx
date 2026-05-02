"use client";

import { useMemo, useState } from "react";
import { useTranslations } from "next-intl";
import { HugeiconsIcon } from "@hugeicons/react";
import { PlusSignIcon } from "@hugeicons/core-free-icons";
import { toast } from "sonner";
import { addWidget } from "@/lib/api";
import { WIDGET_TYPES } from "@/components/dashboard/widgets/widget-types";
import type { DashboardWidget } from "@/types/api";

// ---------------------------------------------------------------------------
// Query templates for quick-start
// ---------------------------------------------------------------------------
const TEMPLATES = [
  {
    key: "countByType" as const,
    query:
      "MATCH (n) RETURN labels(n)[0] AS type, count(n) AS count ORDER BY count DESC",
  },
  {
    key: "topNodes" as const,
    query: "MATCH (n) RETURN n.name AS name, labels(n)[0] AS type LIMIT 10",
  },
  {
    key: "relationshipDistribution" as const,
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
  const t = useTranslations("workbench.dashboard.addWidget");
  const tCommon = useTranslations("common");
  const [open, setOpen] = useState(false);
  const [title, setTitle] = useState("");
  const [query, setQuery] = useState("");
  const [widgetType, setWidgetType] = useState("table");
  const [isSaving, setIsSaving] = useState(false);

  const templates = useMemo(
    () => TEMPLATES.map((tpl) => ({ ...tpl, label: t(`templates.${tpl.key}`) })),
    [t],
  );

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
      toast.success(t("toast.added"));
    } catch {
      toast.error(t("toast.addFailed"));
    } finally {
      setIsSaving(false);
    }
  };

  return (
    <>
      {/* Trigger button */}
      <button
        onClick={() => setOpen(true)}
        className="flex w-full items-center justify-center gap-2 rounded-lg border-2 border-dashed border-divider py-6 text-xs text-muted-foreground transition-colors hover:border-brand-border hover:text-brand-foreground dark:hover:border-brand-foreground"
      >
        <HugeiconsIcon icon={PlusSignIcon} className="h-4 w-4" size="100%" />
        {t("trigger")}
      </button>

      {/* Modal overlay */}
      {open && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-sm">
          <div
            className="w-full max-w-lg rounded-xl border border-divider bg-surface-base p-6 shadow-xl"
            onKeyDown={(e) => {
              if (e.key === "Escape") {
                setOpen(false);
                resetForm();
              }
            }}
          >
            {/* Header */}
            <h3 className="text-sm font-semibold text-foreground-strong">
              {t("modalTitle")}
            </h3>
            <p className="mt-1 text-xs text-muted-foreground">
              {t("modalDescription")}
            </p>

            <div className="mt-4 space-y-4">
              {/* Title */}
              <div>
                <label className="text-2xs font-semibold uppercase tracking-wider text-muted-foreground">
                  {t("titleLabel")}
                </label>
                <input
                  value={title}
                  onChange={(e) => setTitle(e.target.value)}
                  placeholder={t("titlePlaceholder")}
                  autoFocus
                  className="mt-1 w-full rounded-md border border-divider bg-surface-base px-3 py-1.5 text-sm text-foreground focus:border-brand-border focus:ring-1 focus:ring-brand-foreground/50 focus:outline-none-muted"
                />
              </div>

              {/* Widget type */}
              <div>
                <label className="text-2xs font-semibold uppercase tracking-wider text-muted-foreground">
                  {t("typeLabel")}
                </label>
                <select
                  value={widgetType}
                  onChange={(e) => setWidgetType(e.target.value)}
                  className="mt-1 w-full rounded-md border border-divider bg-surface-base px-3 py-1.5 text-sm text-foreground focus:border-brand-border focus:ring-1 focus:ring-brand-foreground/50 focus:outline-none-muted"
                >
                  {WIDGET_TYPES.map((wt) => (
                    <option key={wt.value} value={wt.value}>
                      {wt.label}
                    </option>
                  ))}
                </select>
              </div>

              {/* Templates */}
              <div>
                <label className="text-2xs font-semibold uppercase tracking-wider text-muted-foreground">
                  {t("templatesLabel")}
                </label>
                <div className="mt-1 flex flex-wrap gap-1.5">
                  {templates.map((tpl) => (
                    <button
                      key={tpl.key}
                      type="button"
                      onClick={() => setQuery(tpl.query)}
                      className="rounded-full border border-divider px-2.5 py-1 text-[11px] text-foreground transition-colors hover:border-brand-border hover:bg-brand-surface hover:text-brand-foreground dark:text-muted-foreground dark:hover:border-brand-foreground dark:hover:bg-brand-surface dark:hover:text-brand-foreground"
                    >
                      {tpl.label}
                    </button>
                  ))}
                </div>
              </div>

              {/* Cypher query */}
              <div>
                <label className="text-2xs font-semibold uppercase tracking-wider text-muted-foreground">
                  {t("queryLabel")}
                </label>
                <textarea
                  value={query}
                  onChange={(e) => setQuery(e.target.value)}
                  placeholder={t("queryPlaceholder")}
                  rows={6}
                  className="mt-1 w-full rounded-md border border-divider bg-surface-base px-3 py-2 font-mono text-xs text-foreground focus:border-brand-border focus:ring-1 focus:ring-brand-foreground/50 focus:outline-none-muted"
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
                className="rounded-lg px-4 py-2 text-sm font-medium text-foreground transition-colors hover:bg-surface-inset dark:text-muted-foreground"
              >
                {tCommon("cancel")}
              </button>
              <button
                onClick={handleSave}
                disabled={!title.trim() || !query.trim() || isSaving}
                className="rounded-lg bg-brand-solid px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-brand-solid disabled:opacity-50"
              >
                {isSaving ? t("submitting") : tCommon("submit")}
              </button>
            </div>
          </div>
        </div>
      )}
    </>
  );
}
