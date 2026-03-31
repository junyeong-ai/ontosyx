"use client";

import { useCallback, useEffect, useState } from "react";
import { useAuth } from "@/lib/use-auth";
import {
  createDashboard,
  deleteDashboard,
  listDashboards,
} from "@/lib/api";
import { Spinner } from "@/components/ui/spinner";
import { HugeiconsIcon } from "@hugeicons/react";
import { Add01Icon, Delete01Icon } from "@hugeicons/core-free-icons";
import { toast } from "sonner";
import { useConfirm } from "@/components/ui/confirm-dialog";
import type { Dashboard } from "@/types/api";

export function DashboardPanel() {
  const { canWrite } = useAuth();
  const confirmDialog = useConfirm();
  const [dashboards, setDashboards] = useState<Dashboard[]>([]);
  const [loading, setLoading] = useState(true);
  const [creating, setCreating] = useState(false);

  const fetchDashboards = useCallback(async () => {
    try {
      const page = await listDashboards({ limit: 50 });
      setDashboards(page.items);
    } catch {
      toast.error("Failed to load dashboards");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchDashboards();
  }, [fetchDashboards]);

  const handleCreate = async () => {
    setCreating(true);
    try {
      const dashboard = await createDashboard({
        name: `Dashboard ${dashboards.length + 1}`,
      });
      setDashboards((prev) => [dashboard, ...prev]);
      toast.success("Dashboard created");
    } catch {
      toast.error("Failed to create dashboard");
    } finally {
      setCreating(false);
    }
  };

  const handleDelete = async (id: string) => {
    const confirmed = await confirmDialog({
      title: "Delete dashboard",
      description: "This will permanently delete the dashboard and all its widgets.",
      confirmLabel: "Delete",
      variant: "danger",
    });
    if (!confirmed) return;

    try {
      await deleteDashboard(id);
      setDashboards((prev) => prev.filter((d) => d.id !== id));
      toast.success("Dashboard deleted");
    } catch {
      toast.error("Failed to delete dashboard");
    }
  };

  if (loading) {
    return (
      <div className="flex items-center justify-center py-20">
        <Spinner size="lg" className="text-emerald-500" />
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col bg-zinc-50/50 p-4 dark:bg-zinc-950">
      <div className="flex items-center justify-between">
        <h2 className="text-sm font-semibold text-zinc-700 dark:text-zinc-300">
          Dashboards
          <span className="ml-2 text-xs font-normal text-zinc-400">
            {dashboards.length}
          </span>
        </h2>
        {canWrite && (
          <button
            onClick={handleCreate}
            disabled={creating}
            className="flex items-center gap-1 rounded-md bg-emerald-600 px-2.5 py-1 text-xs font-medium text-white transition-colors hover:bg-emerald-700 disabled:opacity-50"
          >
            <HugeiconsIcon icon={Add01Icon} className="h-3 w-3" size="100%" />
            New
          </button>
        )}
      </div>

      {dashboards.length === 0 ? (
        <div className="flex flex-1 items-center justify-center">
          <div className="text-center">
            <p className="text-sm text-zinc-500">No dashboards yet</p>
            <p className="mt-1 text-xs text-zinc-400">
              Create a dashboard to save and monitor your queries and analyses.
            </p>
          </div>
        </div>
      ) : (
        <div className="mt-3 grid gap-2 overflow-y-auto">
          {dashboards.map((d) => (
            <div
              key={d.id}
              className="flex items-center justify-between rounded-lg border border-zinc-200 bg-white px-4 py-3 dark:border-zinc-800 dark:bg-zinc-900"
            >
              <div>
                <h3 className="text-sm font-medium text-zinc-800 dark:text-zinc-200">
                  {d.name}
                </h3>
                {d.description && (
                  <p className="mt-0.5 text-xs text-zinc-500">{d.description}</p>
                )}
                <p className="mt-0.5 text-[10px] text-zinc-400">
                  Updated {new Date(d.updated_at).toLocaleDateString()}
                </p>
              </div>
              {canWrite && (
                <button
                  onClick={() => handleDelete(d.id)}
                  className="rounded p-1 text-zinc-400 transition-colors hover:bg-red-50 hover:text-red-500 dark:hover:bg-red-950"
                  aria-label="Delete dashboard"
                >
                  <HugeiconsIcon icon={Delete01Icon} className="h-3.5 w-3.5" size="100%" />
                </button>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
