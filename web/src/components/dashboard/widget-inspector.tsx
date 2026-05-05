"use client";

import { useEffect, useState } from "react";
import { useTranslations } from "next-intl";
import { toast } from "@/components/ui/toast";
import { useUpdateWidget } from "@/hooks/api/use-widgets";
import { WIDGET_TYPES } from "@/components/dashboard/widgets/widget-types";
import { Button } from "@/components/ui/button";
import { FormInput, FormTextarea, SettingsSelect } from "@/components/ui/form-input";
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
  const t = useTranslations("workbench.dashboard.inspector");
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
  }, [widget.title, widget.widget_type, widget.query, widget.refresh_interval_secs, widget.thresholds]);

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

  const updateMutation = useUpdateWidget();

  const handleSave = async () => {
    setIsSaving(true);
    try {
      await updateMutation.mutateAsync({
        dashboardId,
        widgetId: widget.id,
        req: {
          title: title !== widget.title ? title : undefined,
          widget_type: widgetType !== widget.widget_type ? widgetType : undefined,
          query: query !== (widget.query ?? "") ? query : undefined,
          refresh_interval_secs:
            refreshSecs !== (widget.refresh_interval_secs ?? 0) ? refreshSecs : undefined,
          thresholds: thresholdsChanged ? thresholds : undefined,
        },
      });
      toast.success(t("toast.updated"));
      onUpdated();
    } catch (err) {
      toast.error(
        t("toast.updateFailed", {
          error: err instanceof Error ? err.message : String(err),
        }),
      );
    } finally {
      setIsSaving(false);
    }
  };

  return (
    <div className="space-y-4">
      <label className="block">
        <span className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
          {t("titleLabel")}
        </span>
        <FormInput
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          className="mt-0.5"
        />
      </label>
      <SettingsSelect
        label={t("chartTypeLabel")}
        value={widgetType}
        onChange={(e) => setWidgetType(e.target.value)}
      >
        {WIDGET_TYPES.map((t) => (
          <option key={t.value} value={t.value}>
            {t.label}
          </option>
        ))}
      </SettingsSelect>
      <label className="block">
        <span className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
          {t("queryLabel")}
        </span>
        <FormTextarea
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          rows={4}
          placeholder={t("queryPlaceholder")}
          className="mt-0.5 bg-surface-raised font-mono text-xs text-brand-foreground"
        />
      </label>
      <label className="block">
        <span className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
          {t("autoRefreshLabel")}
        </span>
        <FormInput
          type="number"
          min={0}
          value={refreshSecs}
          onChange={(e) => setRefreshSecs(parseInt(e.target.value, 10) || 0)}
          placeholder={t("autoRefreshPlaceholder")}
          className="mt-0.5"
        />
        <p className="mt-0.5 text-2xs text-foreground-muted">{t("autoRefreshHint")}</p>
      </label>
      <div>
        <span className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
          {t("thresholdsLabel")}
        </span>
        <div className="mt-0.5 grid grid-cols-2 gap-2">
          <label className="block">
            <span className="text-2xs text-foreground-muted">{t("warningLabel")}</span>
            <FormInput
              type="number"
              value={thresholds.warning ?? ""}
              onChange={(e) =>
                setThresholds((prev) => ({
                  ...prev,
                  warning: e.target.value ? Number(e.target.value) : undefined,
                }))
              }
              placeholder={t("warningPlaceholder")}
            />
          </label>
          <label className="block">
            <span className="text-2xs text-foreground-muted">{t("criticalLabel")}</span>
            <FormInput
              type="number"
              value={thresholds.critical ?? ""}
              onChange={(e) =>
                setThresholds((prev) => ({
                  ...prev,
                  critical: e.target.value ? Number(e.target.value) : undefined,
                }))
              }
              placeholder={t("criticalPlaceholder")}
            />
          </label>
        </div>
        <SettingsSelect
          label={t("thresholdsLabel")}
          hideLabel
          value={thresholds.direction ?? "above"}
          onChange={(e) =>
            setThresholds((prev) => ({
              ...prev,
              direction: e.target.value as "above" | "below",
            }))
          }
          className="mt-1"
        >
          <option value="above">{t("directionAbove")}</option>
          <option value="below">{t("directionBelow")}</option>
        </SettingsSelect>
      </div>
      {hasChanges && (
        <Button
          variant="primary"
          size="sm"
          onClick={handleSave}
          loading={isSaving}
          className="w-full"
        >
          {t("saveButton")}
        </Button>
      )}
    </div>
  );
}
