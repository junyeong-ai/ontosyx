"use client";

import { useCallback, useEffect, useMemo, useState } from "react";
import dynamic from "next/dynamic";
import { useTranslations } from "next-intl";
import { useQueryClient } from "@tanstack/react-query";
import { useAppStore, selectStateSelectedWidgetId } from "@/lib/store";
import { Group, Panel } from "react-resizable-panels";
import { ResizeHandle } from "@/components/ui/resize-handle";
import { Download, LayoutDashboard, Repeat } from "lucide-react";
import { Copy, Network, Share2, Trash } from "lucide-react";
import { SkeletonWidgetGrid } from "@/components/ui/skeleton";
import { Button } from "@/components/ui/button";
import { IconButton } from "@/components/ui/icon-button";
import { Modal } from "@/components/ui/modal";
import { FormInput, FormTextarea } from "@/components/ui/form-input";
import { EmptyState } from "@/components/ui/empty-state";
import { ErrorBoundary } from "@/components/ui/error-boundary";
import { RouteHeading } from "@/components/layout/route-heading";
import { toast } from "@/components/ui/toast";
import {
  createDashboard,
  deleteDashboard,
  updateDashboard,
  listWidgets,
  addWidget,
} from "@/lib/api";
import { dashboardsKeys, useDashboards } from "@/hooks/api/use-dashboards";
import { widgetsKeys, useWidgets } from "@/hooks/api/use-widgets";
import { DashboardAiDialog } from "./dashboard-ai-dialog";
import { InsightListPanel } from "@/components/workbench/insights/insight-list-panel";
// `WidgetGrid` pulls in `react-grid-layout` (~50kb gzipped) and
// every `recharts`-backed chart widget (~120kb gzipped). Splitting
// the chunk via `next/dynamic` keeps both off the critical path
// for users who never mount this layout — workspace setup,
// settings flows, and the design / analyze / explore modes never
// see the dashboard chunks. `ssr: false` because react-grid-layout
// reads window measurements during construction. We reuse
// `SkeletonWidgetGrid` as the loading state so the chunk-resolution
// gap reads as a continuation of the same widget-loading skeleton
// the page already shows on initial fetch — visual register stays
// stable, no flash of "loading dashboard" text.
const WidgetGrid = dynamic(
  () => import("./widget-grid").then((m) => m.WidgetGrid),
  {
    ssr: false,
    loading: () => <SkeletonWidgetGrid count={4} />,
  },
);
import { WidgetInspector } from "./widget-inspector";
import { AddWidgetButton } from "./add-widget-button";

// ---------------------------------------------------------------------------
// Dashboard layout — Action toolbar + Grid (center) | Widget Inspector (right)
// Dashboard selection is handled by ContextSelector in the header.
// ---------------------------------------------------------------------------

export function DashboardLayout() {
  const t = useTranslations("workbench.dashboard.layout");
  const activeDashboardId = useAppStore((s) => s.activeDashboardId);
  const setActiveDashboardId = useAppStore((s) => s.setActiveDashboardId);
  const selectedWidgetId = useAppStore(selectStateSelectedWidgetId);
  const dashboardFilters = useAppStore((s) => s.dashboardFilters);
  const [isAiDialogOpen, setIsAiDialogOpen] = useState(false);
  const [isCreateDialogOpen, setIsCreateDialogOpen] = useState(false);
  const [refreshKey, setRefreshKey] = useState(0);

  const qc = useQueryClient();

  const {
    data: dashboardsPage,
    isLoading,
    isError: dashboardsError,
  } = useDashboards({ limit: 50 });
  const dashboards = useMemo(
    () => dashboardsPage?.items ?? [],
    [dashboardsPage],
  );

  useEffect(() => {
    if (dashboardsError) toast.error(t("toast.loadDashboardsFailed"));
  }, [dashboardsError, t]);

  const {
    data: widgetsData,
    isError: widgetsError,
  } = useWidgets(activeDashboardId);
  const widgets = useMemo(() => widgetsData ?? [], [widgetsData]);

  useEffect(() => {
    if (widgetsError) toast.error(t("toast.loadWidgetsFailed"));
  }, [widgetsError, t]);

  // Auto-select the first dashboard when none is active.
  useEffect(() => {
    if (!activeDashboardId && dashboards.length > 0) {
      setActiveDashboardId(dashboards[0].id);
    }
  }, [activeDashboardId, dashboards, setActiveDashboardId]);

  const refreshWidgets = useCallback(() => {
    if (!activeDashboardId) return;
    qc.invalidateQueries({ queryKey: widgetsKeys.list(activeDashboardId) });
  }, [qc, activeDashboardId]);

  const activeDashboard = dashboards.find((d) => d.id === activeDashboardId);
  const selectedWidget = widgets.find((w) => w.id === selectedWidgetId);

  const handleCreate = () => setIsCreateDialogOpen(true);

  const handleCreateConfirm = async (name: string, description?: string) => {
    try {
      const dash = await createDashboard({
        name,
        description: description || undefined,
      });
      qc.invalidateQueries({ queryKey: dashboardsKeys.lists() });
      setActiveDashboardId(dash.id);
      setIsCreateDialogOpen(false);
      toast.success(t("toast.created"));
    } catch {
      toast.error(t("toast.createFailed"));
    }
  };

  const handleDuplicate = async () => {
    if (!activeDashboard) return;
    try {
      const copy = await createDashboard({
        name: `${activeDashboard.name}${t("create.duplicateSuffix")}`,
        description: activeDashboard.description ?? undefined,
      });
      const srcWidgets = await listWidgets(activeDashboard.id);
      for (const w of srcWidgets) {
        await addWidget(copy.id, {
          title: w.title,
          widget_type: w.widget_type,
          query: w.query ?? undefined,
          widget_spec: w.widget_spec,
          position: w.position,
          refresh_interval_secs: w.refresh_interval_secs ?? undefined,
        });
      }
      qc.invalidateQueries({ queryKey: dashboardsKeys.lists() });
      setActiveDashboardId(copy.id);
      toast.success(t("toast.duplicated"));
    } catch {
      toast.error(t("toast.duplicateFailed"));
    }
  };

  const handleDelete = async (id: string) => {
    if (activeDashboardId === id) {
      setActiveDashboardId(null);
    }
    try {
      await deleteDashboard(id);
      qc.invalidateQueries({ queryKey: dashboardsKeys.lists() });
      toast.success(t("toast.deleted"));
    } catch {
      if (activeDashboardId === id) setActiveDashboardId(id);
      toast.error(t("toast.deleteFailed"));
    }
  };

  const handleToggleSharing = async () => {
    if (!activeDashboardId || !activeDashboard) return;
    const newPublic = !activeDashboard.is_public;
    try {
      await updateDashboard(activeDashboardId, { is_public: newPublic });
      qc.invalidateQueries({ queryKey: dashboardsKeys.lists() });
      toast.success(newPublic ? t("toast.shared") : t("toast.madePrivate"));
    } catch {
      toast.error(t("toast.shareFailed"));
    }
  };

  const handleExportPdf = () => {
    const grid = document.querySelector("[data-dashboard-grid]");
    if (!grid) return;
    const printWindow = window.open("", "_blank");
    if (!printWindow) return;
    const docTitle = activeDashboard?.name ?? "";
    const printDoc = printWindow.document;

    // Build the printable document via DOM APIs — interpolating
    // user-controlled `docTitle` into a template string would inject
    // closing tags / scripts (the dashboard name is operator-supplied
    // free text). `textContent` and direct property writes treat the
    // string as data, never as markup.
    printDoc.title = docTitle;
    const style = printDoc.createElement("style");
    style.textContent =
      "body{font-family:system-ui;padding:20px}" +
      ".widget{border:1px solid #e4e4e7;border-radius:8px;padding:12px;margin:8px;break-inside:avoid}" +
      ".widget-title{font-size:12px;font-weight:600;margin-bottom:8px;color:#3f3f46}";
    printDoc.head.appendChild(style);

    const heading = printDoc.createElement("h1");
    heading.style.fontSize = "18px";
    heading.style.marginBottom = "16px";
    heading.textContent = docTitle;
    printDoc.body.appendChild(heading);

    // Cloning the live grid into the print document is safer than
    // round-tripping through `innerHTML` — preserves React's escaping
    // and avoids re-parsing.
    printDoc.body.appendChild(printDoc.importNode(grid, true));

    printDoc.close();
    printWindow.print();
  };

  if (isLoading) {
    return (
      <div className="p-4">
        <RouteHeading route="dashboard" />
        <SkeletonWidgetGrid count={4} />
      </div>
    );
  }

  return (
    <ErrorBoundary name="Dashboard">
    <RouteHeading route="dashboard" />
    <Group orientation="horizontal" className="h-full">
      {/* Left: Saved insights — author-curated re-runnable artefacts. */}
      <Panel defaultSize="20%" minSize="14%" maxSize="32%">
        <div className="h-full border-e border-divider">
          <InsightListPanel />
        </div>
      </Panel>

      <ResizeHandle />

      {/* Main: Action toolbar + Widget grid */}
      <Panel minSize="40%">
        <div className="flex h-full flex-col">
          <div className="flex h-10 shrink-0 items-center gap-1 border-b border-divider px-4">
            {activeDashboard && (
              <>
                <IconButton
                  label={t("actions.aiGenerate")}
                  onClick={() => setIsAiDialogOpen(true)}
                  icon={Network}
                  tone="brand"
                />
                <IconButton
                  label={
                    activeDashboard.is_public
                      ? t("actions.makePrivate")
                      : t("actions.share")
                  }
                  onClick={handleToggleSharing}
                  active={activeDashboard.is_public}
                  icon={Share2}
                />
                <IconButton
                  label={t("actions.exportPdf")}
                  onClick={handleExportPdf}
                  icon={Download}
                />
                <IconButton
                  label={t("actions.duplicate")}
                  onClick={handleDuplicate}
                  icon={Copy}
                />
                <IconButton
                  label={t("actions.refreshAll")}
                  onClick={() => setRefreshKey((prev) => prev + 1)}
                  icon={Repeat}
                />
                <IconButton
                  label={t("actions.delete")}
                  onClick={() => handleDelete(activeDashboard.id)}
                  icon={Trash}
                  tone="danger"
                />
              </>
            )}
          </div>

          {/* Cross-filter badge bar */}
          {Object.keys(dashboardFilters).length > 0 && (
            <div className="flex items-center gap-2 border-b border-divider px-4 py-2">
              <span className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
                {t("actions.filtersLabel")}
              </span>
              {Object.entries(dashboardFilters).map(([key, value]) => (
                <button
                  type="button"
                  key={key}
                  onClick={() => {
                    const next = { ...dashboardFilters };
                    delete next[key];
                    useAppStore.getState().clearDashboardFilters();
                    for (const [k, v] of Object.entries(next)) {
                      useAppStore.getState().setDashboardFilter(k, v);
                    }
                  }}
                  className="flex items-center gap-1 rounded-full bg-brand-surface px-2.5 py-0.5 text-2xs text-brand-foreground hover:bg-brand-surface-strong/30"
                >
                  {key}: {String(value)}
                  <span className="ms-0.5 text-brand-foreground">&times;</span>
                </button>
              ))}
              <Button
                variant="ghost"
                size="xs"
                onClick={() => useAppStore.getState().clearDashboardFilters()}
              >
                {t("actions.clearAllFilters")}
              </Button>
            </div>
          )}

          {/* Widget grid */}
          <div className="flex-1 overflow-auto p-4">
            {!activeDashboard ? (
              <EmptyDashboard onCreate={handleCreate} />
            ) : (
              <div className="space-y-4" data-dashboard-grid>
                {widgets.length > 0 && (
                  <WidgetGrid
                    widgets={widgets}
                    selectedWidgetId={selectedWidgetId}
                    refreshKey={refreshKey}
                    onSelect={(id) =>
                      useAppStore.getState().selectOne({ kind: "widget", id: id })
                    }
                    onLayoutChange={(newLayout) => {
                      if (!activeDashboardId) return;
                      updateDashboard(activeDashboardId, { layout: newLayout }).catch(
                        () => {
                          /* non-critical: layout persistence */
                        },
                      );
                    }}
                  />
                )}
                {widgets.length === 0 && <EmptyWidgets />}
                <AddWidgetButton
                  dashboardId={activeDashboard.id}
                  existingWidgets={widgets}
                  onAdded={() => {
                    qc.invalidateQueries({
                      queryKey: widgetsKeys.list(activeDashboard.id),
                    });
                  }}
                />
              </div>
            )}
          </div>
        </div>
      </Panel>

      <ResizeHandle />

      {/* Right: Widget Inspector */}
      <Panel defaultSize="25%" minSize="15%" maxSize="40%">
        <div className="flex h-full flex-col border-s border-divider">
          <div className="flex h-10 shrink-0 items-center border-b border-divider px-3">
            <span className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
              {t("inspector.heading")}
            </span>
          </div>
          <div className="flex-1 overflow-auto p-3">
            {selectedWidget ? (
              <WidgetInspector
                widget={selectedWidget}
                dashboardId={activeDashboard?.id ?? ""}
                onUpdated={refreshWidgets}
              />
            ) : (
              <EmptyState variant="compact" title={t("empty.selectWidget")} />
            )}
          </div>
        </div>
      </Panel>

      {/* AI Widget Generator slide-over dialog */}
      {activeDashboardId && (
        <DashboardAiDialog
          open={isAiDialogOpen}
          onClose={() => setIsAiDialogOpen(false)}
          dashboardId={activeDashboardId}
          onWidgetAdded={() => {
            qc.invalidateQueries({
              queryKey: widgetsKeys.list(activeDashboardId),
            });
          }}
        />
      )}

      {isCreateDialogOpen && (
        <CreateDashboardDialog
          defaultName={t("create.defaultName", { number: dashboards.length + 1 })}
          onConfirm={handleCreateConfirm}
          onCancel={() => setIsCreateDialogOpen(false)}
        />
      )}
    </Group>
    </ErrorBoundary>
  );
}

// ---------------------------------------------------------------------------
// Empty states
// ---------------------------------------------------------------------------

function EmptyDashboard({ onCreate }: { onCreate: () => void }) {
  const t = useTranslations("workbench.dashboard.layout.empty");
  return (
    <EmptyState
      icon={LayoutDashboard}
      title={t("dashboardTitle")}
      description={t("dashboardDescription")}
      action={{
        label: t("dashboardCta"),
        onClick: onCreate,
      }}
    />
  );
}

function EmptyWidgets() {
  const t = useTranslations("workbench.dashboard.layout.empty");
  return (
    <EmptyState
      title={t("widgetsTitle")}
      description={t("widgetsDescription")}
      hint={t("widgetsHint")}
    />
  );
}

// ---------------------------------------------------------------------------
// Create Dashboard dialog
// ---------------------------------------------------------------------------

interface CreateDashboardDialogProps {
  defaultName: string;
  onConfirm: (name: string, description?: string) => void;
  onCancel: () => void;
}

function CreateDashboardDialog({
  defaultName,
  onConfirm,
  onCancel,
}: CreateDashboardDialogProps) {
  const t = useTranslations("workbench.dashboard.layout.create");
  const [name, setName] = useState(defaultName);
  const [description, setDescription] = useState("");

  const submit = () => {
    if (!name.trim()) return;
    onConfirm(name.trim(), description.trim() || undefined);
  };

  return (
    <Modal
      open
      onOpenChange={(o) => !o && onCancel()}
      title={t("title")}
      size="sm"
      footer={
        <>
          <Button variant="ghost" size="sm" onClick={onCancel}>
            {t("cancel")}
          </Button>
          <Button
            variant="primary"
            size="sm"
            onClick={submit}
            disabled={!name.trim()}
          >
            {t("submit")}
          </Button>
        </>
      }
    >
      <div className="space-y-3">
        <label className="block">
          <span className="mb-1 block text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
            {t("nameLabel")}
          </span>
          <FormInput
            value={name}
            onChange={(e) => setName(e.target.value)}
            autoFocus
            placeholder={t("namePlaceholder")}
            onKeyDown={(e) => {
              if (e.key === "Enter") submit();
              if (e.key === "Escape") onCancel();
            }}
          />
        </label>
        <label className="block">
          <span className="mb-1 block text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
            {t("descriptionLabel")}
          </span>
          <FormTextarea
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            rows={2}
            placeholder={t("descriptionPlaceholder")}
          />
        </label>
      </div>
    </Modal>
  );
}

