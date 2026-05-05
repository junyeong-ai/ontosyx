"use client";

import { useMemo, useState } from "react";
import { useTranslations } from "next-intl";
import { Plus } from "lucide-react";
import { toast } from "@/components/ui/toast";
import { Heading } from "@/components/ui/heading";
import { addWidget } from "@/lib/api";
import { WIDGET_TYPES } from "@/components/dashboard/widgets/widget-types";
import { Button } from "@/components/ui/button";
import { FormInput, FormTextarea, SettingsSelect } from "@/components/ui/form-input";
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
        type="button"
        onClick={() => setOpen(true)}
        className="flex w-full items-center justify-center gap-2 rounded-lg border-2 border-dashed border-divider py-6 text-xs text-foreground-muted transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] hover:border-brand-border hover:text-brand-foreground"
      >
        <Plus className="h-4 w-4" />
        {t("trigger")}
      </button>

      {/* Modal overlay */}
      {open && (
        <div className="fixed inset-0 z-modal flex items-center justify-center bg-surface-scrim-strong backdrop-blur-sm">
          <div
            className="w-full max-w-lg rounded-xl border border-divider bg-surface-base p-6 shadow-4"
            onKeyDown={(e) => {
              if (e.key === "Escape") {
                setOpen(false);
                resetForm();
              }
            }}
          >
            {/* Header */}
            <Heading level={3} size={6}>
              {t("modalTitle")}
            </Heading>
            <p className="mt-1 text-xs text-foreground-muted">
              {t("modalDescription")}
            </p>

            <div className="mt-4 space-y-4">
              <label className="block">
                <span className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
                  {t("titleLabel")}
                </span>
                <FormInput
                  value={title}
                  onChange={(e) => setTitle(e.target.value)}
                  placeholder={t("titlePlaceholder")}
                  autoFocus
                  className="mt-1"
                />
              </label>

              <SettingsSelect
                label={t("typeLabel")}
                value={widgetType}
                onChange={(e) => setWidgetType(e.target.value)}
              >
                {WIDGET_TYPES.map((wt) => (
                  <option key={wt.value} value={wt.value}>
                    {wt.label}
                  </option>
                ))}
              </SettingsSelect>

              <div>
                <span className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
                  {t("templatesLabel")}
                </span>
                <div className="mt-1 flex flex-wrap gap-1.5">
                  {templates.map((tpl) => (
                    <button
                      key={tpl.key}
                      type="button"
                      onClick={() => setQuery(tpl.query)}
                      className="rounded-full border border-divider px-2.5 py-1 text-2xs text-foreground transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] hover:border-brand-border hover:bg-brand-surface hover:text-brand-foreground"
                    >
                      {tpl.label}
                    </button>
                  ))}
                </div>
              </div>

              <label className="block">
                <span className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
                  {t("queryLabel")}
                </span>
                <FormTextarea
                  value={query}
                  onChange={(e) => setQuery(e.target.value)}
                  placeholder={t("queryPlaceholder")}
                  rows={6}
                  className="mt-1 font-mono text-xs"
                />
              </label>
            </div>

            <div className="mt-5 flex justify-end gap-2">
              <Button
                variant="ghost"
                size="md"
                onClick={() => {
                  setOpen(false);
                  resetForm();
                }}
              >
                {tCommon("cancel")}
              </Button>
              <Button
                variant="primary"
                size="md"
                onClick={handleSave}
                disabled={!title.trim() || !query.trim()}
                loading={isSaving}
              >
                {tCommon("submit")}
              </Button>
            </div>
          </div>
        </div>
      )}
    </>
  );
}
