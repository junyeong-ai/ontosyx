"use client";

import { useEffect, useState } from "react";
import { useTranslations } from "next-intl";
import { Spinner } from "@/components/ui/spinner";
import { toast } from "sonner";
import { useConfirm } from "@/components/ui/confirm-dialog";
import type { ScheduledTask } from "@/types/api";
import {
  listScheduledTasks,
  updateScheduledTask,
  deleteScheduledTask,
} from "@/lib/api";

const STATUS_BADGE: Record<string, string> = {
  completed:
    "bg-emerald-100 text-emerald-700 dark:bg-emerald-900/30 dark:text-emerald-400",
  error:
    "bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400",
  running:
    "bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-400",
};

export default function SchedulesPage() {
  const t = useTranslations("settings.schedules");
  const commonT = useTranslations("common");
  const [tasks, setTasks] = useState<ScheduledTask[]>([]);
  const [loading, setLoading] = useState(true);
  const confirm = useConfirm();

  useEffect(() => {
    listScheduledTasks()
      .then((items) => setTasks(items))
      .catch(() => toast.error(t("toast.loadFailed")))
      .finally(() => setLoading(false));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const handleToggle = async (task: ScheduledTask) => {
    const newEnabled = !task.enabled;
    // Optimistic update
    setTasks((prev) =>
      prev.map((tt) => (tt.id === task.id ? { ...tt, enabled: newEnabled } : tt)),
    );
    try {
      await updateScheduledTask(task.id, { enabled: newEnabled });
      toast.success(newEnabled ? t("toast.enabled") : t("toast.disabled"));
    } catch {
      // Revert
      setTasks((prev) =>
        prev.map((tt) =>
          tt.id === task.id ? { ...tt, enabled: task.enabled } : tt,
        ),
      );
      toast.error(t("toast.updateFailed"));
    }
  };

  const handleDelete = async (id: string) => {
    const task = tasks.find((tt) => tt.id === id);
    const ok = await confirm({
      title: t("deleteConfirm.title", {
        name: task?.description ?? id,
      }),
      description: t("deleteConfirm.description"),
      variant: "danger",
    });
    if (!ok) return;
    const snapshot = tasks;
    setTasks((prev) => prev.filter((tt) => tt.id !== id));
    try {
      await deleteScheduledTask(id);
      toast.success(t("toast.deleted"));
    } catch {
      setTasks(snapshot);
      toast.error(t("toast.deleteFailed"));
    }
  };

  if (loading) {
    return (
      <div className="flex items-center justify-center py-12">
        <Spinner size="lg" />
      </div>
    );
  }

  return (
    <div>
      <h1 className="text-lg font-semibold text-zinc-800 dark:text-zinc-200">
        {t("title")}
      </h1>
      <p className="mt-1 text-sm text-muted-foreground">
        {t("description")}
      </p>

      {tasks.length === 0 ? (
        <p className="mt-6 text-sm text-muted-foreground">
          {t("empty")}
        </p>
      ) : (
        <div className="mt-6 overflow-hidden rounded-lg border border-zinc-200 dark:border-zinc-700">
          <table className="w-full text-left text-xs">
            <thead>
              <tr className="border-b border-zinc-200 bg-zinc-50 dark:border-zinc-700 dark:bg-zinc-900">
                <th className="py-3 pr-6 font-semibold text-muted-foreground">
                  {t("column.description")}
                </th>
                <th className="py-3 pr-6 font-semibold text-muted-foreground">
                  {t("column.cron")}
                </th>
                <th className="py-3 pr-6 font-semibold text-muted-foreground">
                  {t("column.status")}
                </th>
                <th className="py-3 pr-6 font-semibold text-muted-foreground">
                  {t("column.lastRun")}
                </th>
                <th className="py-3 pr-6 font-semibold text-muted-foreground">
                  {t("column.nextRun")}
                </th>
                <th className="py-3 pr-6 font-semibold text-muted-foreground">
                  {t("column.enabled")}
                </th>
                <th className="py-3 pr-6 font-semibold text-muted-foreground" />
              </tr>
            </thead>
            <tbody className="divide-y divide-zinc-200 dark:divide-zinc-700">
              {tasks.map((task) => (
                <tr
                  key={task.id}
                  className="bg-white dark:bg-zinc-950 hover:bg-zinc-50 dark:hover:bg-zinc-900"
                >
                  <td className="py-3 pr-6 text-zinc-700 dark:text-zinc-300">
                    {task.description ?? task.recipe_id.slice(0, 8)}
                  </td>
                  <td className="py-3 pr-6 font-mono text-muted-foreground">
                    {task.cron_expression}
                  </td>
                  <td className="py-3 pr-6">
                    {task.last_status ? (
                      <span
                        className={`inline-flex rounded-full px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wider ${STATUS_BADGE[task.last_status] ?? "bg-zinc-100 text-muted-foreground"}`}
                      >
                        {task.last_status}
                      </span>
                    ) : (
                      <span className="text-muted-foreground">--</span>
                    )}
                  </td>
                  <td className="py-3 pr-6 text-muted-foreground">
                    {task.last_run_at
                      ? new Date(task.last_run_at).toLocaleString()
                      : "--"}
                  </td>
                  <td className="py-3 pr-6 text-muted-foreground">
                    {new Date(task.next_run_at).toLocaleString()}
                  </td>
                  <td className="py-3 pr-6">
                    <button
                      onClick={() => handleToggle(task)}
                      className={`relative inline-flex h-5 w-9 items-center rounded-full transition-colors ${
                        task.enabled
                          ? "bg-emerald-500"
                          : "bg-zinc-300 dark:bg-zinc-600"
                      }`}
                      aria-label={
                        task.enabled ? t("disableAria") : t("enableAria")
                      }
                    >
                      <span
                        className={`inline-block h-3.5 w-3.5 rounded-full bg-white transition-transform ${
                          task.enabled ? "translate-x-4.5" : "translate-x-0.5"
                        }`}
                      />
                    </button>
                  </td>
                  <td className="py-3 pr-6">
                    <button
                      onClick={() => handleDelete(task.id)}
                      className="rounded-md px-2 py-1 text-[10px] font-medium text-red-600 hover:bg-red-50 dark:hover:bg-red-950"
                    >
                      {commonT("delete")}
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
