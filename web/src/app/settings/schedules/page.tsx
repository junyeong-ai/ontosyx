"use client";

import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { useTranslations } from "next-intl";
import { toast } from "sonner";
import { Calendar01Icon } from "@hugeicons/core-free-icons";

import { SettingsPageShell } from "@/components/layout/settings-page-shell";
import { EmptyState } from "@/components/ui/empty-state";
import { ErrorState } from "@/components/ui/error-state";
import { SkeletonTable } from "@/components/ui/skeleton";
import { useConfirm } from "@/components/providers/confirm-provider";
import type { ScheduledTask } from "@/types/api";
import {
  listScheduledTasks,
  updateScheduledTask,
  deleteScheduledTask,
} from "@/lib/api";

const STATUS_BADGE: Record<string, string> = {
  completed: "bg-success-surface text-success-foreground",
  error: "bg-danger-surface text-danger-foreground",
  running: "bg-info-surface text-info-foreground",
};

const schedulesKeys = {
  all: ["schedules"] as const,
  list: () => [...schedulesKeys.all, "list"] as const,
};

export default function SchedulesPage() {
  const t = useTranslations("settings.schedules");
  const tCommon = useTranslations("common");
  const confirm = useConfirm();
  const qc = useQueryClient();

  const query = useQuery({
    queryKey: schedulesKeys.list(),
    queryFn: () => listScheduledTasks(),
  });

  const toggleMutation = useMutation({
    mutationFn: ({ id, enabled }: { id: string; enabled: boolean }) =>
      updateScheduledTask(id, { enabled }),
    onMutate: async ({ id, enabled }) => {
      await qc.cancelQueries({ queryKey: schedulesKeys.list() });
      const previous = qc.getQueryData<ScheduledTask[]>(schedulesKeys.list());
      if (previous) {
        qc.setQueryData<ScheduledTask[]>(
          schedulesKeys.list(),
          previous.map((tt) => (tt.id === id ? { ...tt, enabled } : tt)),
        );
      }
      return { previous };
    },
    onError: (_err, _vars, context) => {
      if (context?.previous) {
        qc.setQueryData(schedulesKeys.list(), context.previous);
      }
      toast.error(t("toast.updateFailed"));
    },
    onSuccess: (_data, { enabled }) => {
      toast.success(enabled ? t("toast.enabled") : t("toast.disabled"));
    },
    onSettled: () => {
      qc.invalidateQueries({ queryKey: schedulesKeys.list() });
    },
  });

  const deleteMutation = useMutation({
    mutationFn: (id: string) => deleteScheduledTask(id),
    onMutate: async (id) => {
      await qc.cancelQueries({ queryKey: schedulesKeys.list() });
      const previous = qc.getQueryData<ScheduledTask[]>(schedulesKeys.list());
      if (previous) {
        qc.setQueryData<ScheduledTask[]>(
          schedulesKeys.list(),
          previous.filter((tt) => tt.id !== id),
        );
      }
      return { previous };
    },
    onError: (_err, _id, context) => {
      if (context?.previous) {
        qc.setQueryData(schedulesKeys.list(), context.previous);
      }
      toast.error(t("toast.deleteFailed"));
    },
    onSuccess: () => toast.success(t("toast.deleted")),
    onSettled: () => {
      qc.invalidateQueries({ queryKey: schedulesKeys.list() });
    },
  });

  const handleDelete = async (id: string) => {
    const tasks = query.data ?? [];
    const task = tasks.find((tt) => tt.id === id);
    const ok = await confirm({
      title: t("deleteConfirm.title", { name: task?.description ?? id }),
      description: t("deleteConfirm.description"),
      variant: "danger",
    });
    if (!ok) return;
    deleteMutation.mutate(id);
  };

  if (query.isLoading) {
    return (
      <SettingsPageShell title={t("title")} subtitle={t("description")}>
        <SkeletonTable rows={5} cols={6} />
      </SettingsPageShell>
    );
  }

  if (query.isError) {
    return (
      <SettingsPageShell title={t("title")} subtitle={t("description")}>
        <ErrorState
          title={tCommon("loadError.title")}
          description={tCommon("loadError.description")}
          onRetry={() => query.refetch()}
          retryLabel={tCommon("retry")}
        />
      </SettingsPageShell>
    );
  }

  const tasks = query.data ?? [];

  return (
    <SettingsPageShell title={t("title")} subtitle={t("description")}>
      {tasks.length === 0 ? (
        <div className="mt-6">
          <EmptyState icon={Calendar01Icon} title={t("empty")} />
        </div>
      ) : (
        <div className="mt-6 overflow-hidden rounded-lg border border-divider">
          <table className="w-full text-left text-xs">
            <thead>
              <tr className="border-b border-divider bg-surface-raised">
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
            <tbody className="divide-y divide-divider">
              {tasks.map((task) => (
                <tr
                  key={task.id}
                  className="bg-surface-base hover:bg-surface-raised"
                >
                  <td className="py-3 pr-6 text-foreground">
                    {task.description ?? task.recipe_id.slice(0, 8)}
                  </td>
                  <td className="py-3 pr-6 font-mono text-muted-foreground">
                    {task.cron_expression}
                  </td>
                  <td className="py-3 pr-6">
                    {task.last_status ? (
                      <span
                        className={`inline-flex rounded-full px-2 py-0.5 text-2xs font-semibold uppercase tracking-wider ${STATUS_BADGE[task.last_status] ?? "bg-surface-inset text-muted-foreground"}`}
                      >
                        {task.last_status}
                      </span>
                    ) : (
                      <span className="text-muted-foreground">—</span>
                    )}
                  </td>
                  <td className="py-3 pr-6 text-muted-foreground">
                    {task.last_run_at
                      ? new Date(task.last_run_at).toLocaleString()
                      : "—"}
                  </td>
                  <td className="py-3 pr-6 text-muted-foreground">
                    {new Date(task.next_run_at).toLocaleString()}
                  </td>
                  <td className="py-3 pr-6">
                    <button
                      onClick={() =>
                        toggleMutation.mutate({
                          id: task.id,
                          enabled: !task.enabled,
                        })
                      }
                      className={`relative inline-flex h-5 w-9 items-center rounded-full transition-colors ${
                        task.enabled
                          ? "bg-brand-solid"
                          : "bg-surface-raised"
                      }`}
                      aria-label={
                        task.enabled ? t("disableAria") : t("enableAria")
                      }
                    >
                      <span
                        className={`inline-block h-3.5 w-3.5 rounded-full bg-surface-base transition-transform ${
                          task.enabled ? "translate-x-4.5" : "translate-x-0.5"
                        }`}
                      />
                    </button>
                  </td>
                  <td className="py-3 pr-6">
                    <button
                      onClick={() => handleDelete(task.id)}
                      className="rounded-md px-2 py-1 text-2xs font-medium text-danger-foreground hover:bg-danger-surface"
                    >
                      {tCommon("delete")}
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </SettingsPageShell>
  );
}
