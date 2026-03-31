"use client";

import { useCallback, useEffect, useState } from "react";
import { useAppStore, selectSelectedWidgetId } from "@/lib/store";
import { Group, Panel } from "react-resizable-panels";
import { ResizeHandle } from "@/components/ui/resize-handle";
import { HugeiconsIcon } from "@hugeicons/react";
import {
  AiNetworkIcon,
  DashboardSpeed01Icon,
  Delete02Icon,
  RepeatIcon,
} from "@hugeicons/core-free-icons";
import { Tooltip } from "@/components/ui/tooltip";
import { SkeletonWidgetGrid } from "@/components/ui/skeleton";
import { toast } from "sonner";
import type { Dashboard, DashboardWidget } from "@/types/api";
import {
  listDashboards,
  createDashboard,
  deleteDashboard,
  updateDashboard,
  listWidgets,
  addWidget,
} from "@/lib/api";
import { DashboardAiDialog } from "./dashboard-ai-dialog";
import { ErrorBoundary } from "@/components/ui/error-boundary";
import { EmptyState } from "@/components/ui/empty-state";
import { WidgetGrid } from "./dashboard/widget-grid";
import { WidgetInspector } from "./dashboard/widget-inspector";
import { AddWidgetButton } from "./dashboard/add-widget-button";

// ---------------------------------------------------------------------------
// Dashboard layout — Action toolbar + Grid (center) | Widget Inspector (right)
// Dashboard selection is handled by ContextSelector in the header.
// ---------------------------------------------------------------------------

export function DashboardLayout() {
  const activeDashboardId = useAppStore((s) => s.activeDashboardId);
  const setActiveDashboardId = useAppStore((s) => s.setActiveDashboardId);
  const selectedWidgetId = useAppStore(selectSelectedWidgetId);
  const dashboardFilters = useAppStore((s) => s.dashboardFilters);
  const [dashboards, setDashboards] = useState<Dashboard[]>([]);
  const [widgets, setWidgets] = useState<DashboardWidget[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [isAiDialogOpen, setIsAiDialogOpen] = useState(false);
  const [isCreateDialogOpen, setIsCreateDialogOpen] = useState(false);
  const [refreshKey, setRefreshKey] = useState(0);

  // Load dashboards on mount (auto-select first if none active)
  useEffect(() => {
    setIsLoading(true);
    listDashboards({ limit: 50 })
      .then((page) => {
        setDashboards(page.items);
        if (!activeDashboardId && page.items.length > 0) {
          setActiveDashboardId(page.items[0].id);
        }
      })
      .catch(() => toast.error("Failed to load dashboards"))
      .finally(() => setIsLoading(false));
  }, []);

  // Load widgets when active dashboard changes + sync count to store
  const refreshWidgets = useCallback(() => {
    if (!activeDashboardId) return;
    listWidgets(activeDashboardId)
      .then((w) => {
        setWidgets(w);
        useAppStore.getState().setDashboardWidgetCount(w.length);
      })
      .catch(() => toast.error("Failed to load widgets"));
  }, [activeDashboardId]);

  useEffect(() => {
    if (!activeDashboardId) {
      setWidgets([]);
      useAppStore.getState().setDashboardWidgetCount(0);
      return;
    }
    refreshWidgets();
  }, [activeDashboardId, refreshWidgets]);

  const handleCreate = () => {
    setIsCreateDialogOpen(true);
  };

  const handleCreateConfirm = async (name: string, description?: string) => {
    try {
      const dash = await createDashboard({
        name,
        description: description || undefined,
      });
      setDashboards((prev) => [dash, ...prev]);
      setActiveDashboardId(dash.id);
      setIsCreateDialogOpen(false);
      toast.success("Dashboard created");
    } catch {
      toast.error("Failed to create dashboard");
    }
  };

  const handleDuplicate = async () => {
    if (!activeDashboard) return;
    try {
      const copy = await createDashboard({
        name: `${activeDashboard.name} (Copy)`,
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
      setDashboards((prev) => [copy, ...prev]);
      setActiveDashboardId(copy.id);
      toast.success("Dashboard duplicated");
    } catch {
      toast.error("Failed to duplicate dashboard");
    }
  };

  const handleDelete = async (id: string) => {
    const snapshot = dashboards;
    setDashboards((prev) => prev.filter((d) => d.id !== id));
    if (activeDashboardId === id) {
      setActiveDashboardId(null);
    }
    try {
      await deleteDashboard(id);
      toast.success("Dashboard deleted");
    } catch {
      setDashboards(snapshot);
      if (activeDashboardId === id) setActiveDashboardId(id);
      toast.error("Failed to delete dashboard");
    }
  };

  if (isLoading) {
    return (
      <div className="p-4">
        <SkeletonWidgetGrid count={4} />
      </div>
    );
  }

  const activeDashboard = dashboards.find((d) => d.id === activeDashboardId);
  const selectedWidget = widgets.find((w) => w.id === selectedWidgetId);

  return (
    <ErrorBoundary name="Dashboard">
    <Group orientation="horizontal" className="h-full">
      {/* Main: Action toolbar + Widget grid */}
      <Panel minSize="50%">
        <div className="flex h-full flex-col">
          {/* Action toolbar (no selector -- that lives in the header) */}
          <div className="flex h-10 shrink-0 items-center gap-2 border-b border-zinc-200 px-4 dark:border-zinc-800">
            {activeDashboard && (
              <>
                <Tooltip content="AI Generate Widgets">
                  <button
                    onClick={() => setIsAiDialogOpen(true)}
                    aria-label="AI Generate Widgets"
                    className="rounded-md p-1 text-zinc-400 hover:bg-emerald-50 hover:text-emerald-600 dark:hover:bg-emerald-950"
                  >
                    <HugeiconsIcon icon={AiNetworkIcon} className="h-3.5 w-3.5" size="100%" />
                  </button>
                </Tooltip>
                <Tooltip content={activeDashboard.is_public ? "Make Private" : "Share Dashboard"}>
                  <button
                    aria-label={activeDashboard.is_public ? "Make Private" : "Share Dashboard"}
                    onClick={async () => {
                      if (!activeDashboardId) return;
                      const newPublic = !activeDashboard.is_public;
                      try {
                        await updateDashboard(activeDashboardId, { is_public: newPublic });
                        setDashboards(prev => prev.map(d =>
                          d.id === activeDashboardId ? { ...d, is_public: newPublic } : d
                        ));
                        toast.success(newPublic ? "Dashboard shared" : "Dashboard made private");
                      } catch {
                        toast.error("Failed to update sharing");
                      }
                    }}
                    className={`rounded-md p-1 ${
                      activeDashboard.is_public
                        ? "text-emerald-500 hover:bg-emerald-50 dark:hover:bg-emerald-950"
                        : "text-zinc-400 hover:bg-zinc-100 dark:hover:bg-zinc-800"
                    }`}
                  >
                    <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" strokeWidth={1.5} stroke="currentColor" className="h-3.5 w-3.5">
                      <path strokeLinecap="round" strokeLinejoin="round" d="M7.217 10.907a2.25 2.25 0 1 0 0 2.186m0-2.186c.18.324.283.696.283 1.093s-.103.77-.283 1.093m0-2.186 9.566-5.314m-9.566 7.5 9.566 5.314m0 0a2.25 2.25 0 1 0 3.935 2.186 2.25 2.25 0 0 0-3.935-2.186Zm0-12.814a2.25 2.25 0 1 0 3.933-2.185 2.25 2.25 0 0 0-3.933 2.185Z" />
                    </svg>
                  </button>
                </Tooltip>
                <Tooltip content="Export as PDF">
                  <button
                    aria-label="Export as PDF"
                    onClick={() => {
                      const grid = document.querySelector("[data-dashboard-grid]");
                      if (!grid) return;
                      const printWindow = window.open("", "_blank");
                      if (!printWindow) return;
                      printWindow.document.write(`
                        <html><head><title>${activeDashboard?.name ?? "Dashboard"}</title>
                        <style>body{font-family:system-ui;padding:20px}
                        .widget{border:1px solid #e4e4e7;border-radius:8px;padding:12px;margin:8px;break-inside:avoid}
                        .widget-title{font-size:12px;font-weight:600;margin-bottom:8px;color:#3f3f46}
                        </style></head><body>
                        <h1 style="font-size:18px;margin-bottom:16px">${activeDashboard?.name ?? "Dashboard"}</h1>
                        ${grid.innerHTML}
                        </body></html>
                      `);
                      printWindow.document.close();
                      printWindow.print();
                    }}
                    className="rounded-md p-1 text-zinc-400 hover:bg-zinc-100 hover:text-zinc-600 dark:hover:bg-zinc-800"
                  >
                    <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" strokeWidth={1.5} stroke="currentColor" className="h-3.5 w-3.5">
                      <path strokeLinecap="round" strokeLinejoin="round" d="M3 16.5v2.25A2.25 2.25 0 0 0 5.25 21h13.5A2.25 2.25 0 0 0 21 18.75V16.5M16.5 12 12 16.5m0 0L7.5 12m4.5 4.5V3" />
                    </svg>
                  </button>
                </Tooltip>
                <Tooltip content="Duplicate Dashboard">
                  <button
                    onClick={handleDuplicate}
                    aria-label="Duplicate Dashboard"
                    className="rounded-md p-1 text-zinc-400 hover:bg-zinc-100 hover:text-zinc-600 dark:hover:bg-zinc-800"
                  >
                    <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" strokeWidth={1.5} stroke="currentColor" className="h-3.5 w-3.5">
                      <path strokeLinecap="round" strokeLinejoin="round" d="M15.75 17.25v3.375c0 .621-.504 1.125-1.125 1.125h-9.75a1.125 1.125 0 0 1-1.125-1.125V7.875c0-.621.504-1.125 1.125-1.125H6.75a9.06 9.06 0 0 1 1.5.124m7.5 10.376h3.375c.621 0 1.125-.504 1.125-1.125V11.25c0-4.46-3.243-8.161-7.5-8.876a9.06 9.06 0 0 0-1.5-.124H9.375c-.621 0-1.125.504-1.125 1.125v3.5m7.5 10.375H9.375a1.125 1.125 0 0 1-1.125-1.125v-9.25m12 6.625v-1.875a3.375 3.375 0 0 0-3.375-3.375h-1.5a1.125 1.125 0 0 1-1.125-1.125v-1.5a3.375 3.375 0 0 0-3.375-3.375H9.75" />
                    </svg>
                  </button>
                </Tooltip>
                <Tooltip content="Refresh All Widgets">
                  <button
                    onClick={() => setRefreshKey((prev) => prev + 1)}
                    aria-label="Refresh All Widgets"
                    className="rounded-md p-1 text-zinc-400 hover:bg-zinc-100 hover:text-zinc-600 dark:hover:bg-zinc-800"
                  >
                    <HugeiconsIcon icon={RepeatIcon} className="h-3.5 w-3.5" size="100%" />
                  </button>
                </Tooltip>
                <Tooltip content="Delete Dashboard">
                  <button
                    onClick={() => handleDelete(activeDashboard.id)}
                    aria-label="Delete Dashboard"
                    className="rounded-md p-1 text-zinc-400 hover:bg-red-50 hover:text-red-500 dark:hover:bg-red-950"
                  >
                    <HugeiconsIcon icon={Delete02Icon} className="h-3.5 w-3.5" size="100%" />
                  </button>
                </Tooltip>
              </>
            )}
          </div>

          {/* Cross-filter badge bar */}
          {Object.keys(dashboardFilters).length > 0 && (
            <div className="flex items-center gap-2 border-b border-zinc-200 px-4 py-2 dark:border-zinc-800">
              <span className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">Filters:</span>
              {Object.entries(dashboardFilters).map(([key, value]) => (
                <button
                  key={key}
                  onClick={() => {
                    const next = { ...dashboardFilters };
                    delete next[key];
                    useAppStore.getState().clearDashboardFilters();
                    Object.entries(next).forEach(([k, v]) => useAppStore.getState().setDashboardFilter(k, v));
                  }}
                  className="flex items-center gap-1 rounded-full bg-emerald-50 px-2.5 py-0.5 text-[11px] text-emerald-700 hover:bg-emerald-100 dark:bg-emerald-950/30 dark:text-emerald-400"
                >
                  {key}: {String(value)}
                  <span className="ml-0.5 text-emerald-400">&times;</span>
                </button>
              ))}
              <button
                onClick={() => useAppStore.getState().clearDashboardFilters()}
                className="text-[10px] text-zinc-400 hover:text-zinc-600"
              >
                Clear all
              </button>
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
                    onSelect={(id) => useAppStore.getState().select({ type: "widget", widgetId: id })}
                    onLayoutChange={(newLayout) => {
                      if (!activeDashboardId) return;
                      updateDashboard(activeDashboardId, { layout: newLayout }).catch(() => { /* non-critical: layout persistence */ });
                    }}
                  />
                )}
                {widgets.length === 0 && <EmptyWidgets />}
                <AddWidgetButton
                  dashboardId={activeDashboard.id}
                  existingWidgets={widgets}
                  onAdded={(w) => {
                    setWidgets((prev) => {
                      const next = [...prev, w];
                      useAppStore.getState().setDashboardWidgetCount(next.length);
                      return next;
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
        <div className="flex h-full flex-col border-l border-zinc-200 dark:border-zinc-800">
          <div className="flex h-10 shrink-0 items-center border-b border-zinc-200 px-3 dark:border-zinc-800">
            <span className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
              Widget Inspector
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
              <p className="text-xs text-zinc-400 text-center mt-8">
                Select a widget to configure
              </p>
            )}
          </div>
        </div>
      </Panel>

      {/* AI Widget Generator slide-over dialog */}
      {activeDashboardId && (
        <DashboardAiDialog
          isOpen={isAiDialogOpen}
          onClose={() => setIsAiDialogOpen(false)}
          dashboardId={activeDashboardId}
          onWidgetAdded={(widget) => {
            setWidgets((prev) => {
              const next = [...prev, widget];
              useAppStore.getState().setDashboardWidgetCount(next.length);
              return next;
            });
          }}
        />
      )}

      {/* Create Dashboard dialog */}
      {isCreateDialogOpen && (
        <CreateDashboardDialog
          defaultName={`Dashboard ${dashboards.length + 1}`}
          onConfirm={handleCreateConfirm}
          onCancel={() => setIsCreateDialogOpen(false)}
        />
      )}
    </Group>
    </ErrorBoundary>
  );
}

// ---------------------------------------------------------------------------
// Small inline sub-components
// ---------------------------------------------------------------------------

function EmptyDashboard({ onCreate }: { onCreate: () => void }) {
  return (
    <EmptyState
      icon={DashboardSpeed01Icon}
      title="No Dashboard Selected"
      description="Create dashboards to monitor your graph data with charts, tables, and KPI cards."
      action={{ label: "Create Dashboard", onClick: onCreate }}
    />
  );
}

function EmptyWidgets() {
  return (
    <EmptyState
      title="No Widgets Yet"
      description="Add widgets manually or use AI to generate them automatically."
      hint="Use the AI button in the toolbar above to auto-generate widgets"
    />
  );
}

function CreateDashboardDialog({
  defaultName,
  onConfirm,
  onCancel,
}: {
  defaultName: string;
  onConfirm: (name: string, description?: string) => void;
  onCancel: () => void;
}) {
  const [name, setName] = useState(defaultName);
  const [description, setDescription] = useState("");

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
      <div className="w-full max-w-sm rounded-lg border border-zinc-200 bg-white p-5 shadow-lg dark:border-zinc-700 dark:bg-zinc-900">
        <h3 className="text-sm font-semibold text-zinc-700 dark:text-zinc-200">
          Create Dashboard
        </h3>
        <div className="mt-3 space-y-3">
          <div>
            <label className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
              Name
            </label>
            <input
              value={name}
              onChange={(e) => setName(e.target.value)}
              autoFocus
              className="mt-0.5 w-full rounded-md border border-zinc-200 bg-white px-3 py-1.5 text-sm text-zinc-700 focus:border-emerald-400 focus:ring-1 focus:ring-emerald-400/50 focus:outline-none dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-300"
              placeholder="Dashboard name"
              onKeyDown={(e) => {
                if (e.key === "Enter" && name.trim()) onConfirm(name.trim(), description.trim() || undefined);
                if (e.key === "Escape") onCancel();
              }}
            />
          </div>
          <div>
            <label className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
              Description
            </label>
            <textarea
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              rows={2}
              className="mt-0.5 w-full rounded-md border border-zinc-200 bg-white px-3 py-1.5 text-sm text-zinc-700 focus:border-emerald-400 focus:ring-1 focus:ring-emerald-400/50 focus:outline-none dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-300"
              placeholder="Optional description"
            />
          </div>
        </div>
        <div className="mt-4 flex justify-end gap-2">
          <button
            onClick={onCancel}
            className="rounded-md px-3 py-1.5 text-xs text-zinc-500 hover:bg-zinc-100 dark:hover:bg-zinc-800"
          >
            Cancel
          </button>
          <button
            onClick={() => name.trim() && onConfirm(name.trim(), description.trim() || undefined)}
            disabled={!name.trim()}
            className="rounded-md bg-emerald-600 px-4 py-1.5 text-xs font-medium text-white hover:bg-emerald-700 disabled:opacity-50"
          >
            Create
          </button>
        </div>
      </div>
    </div>
  );
}
