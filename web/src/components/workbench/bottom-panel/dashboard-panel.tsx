"use client";

import { useTranslations } from "next-intl";
import { useAuth } from "@/hooks/use-auth";
import { Spinner } from "@/components/ui/spinner";
import { HugeiconsIcon } from "@hugeicons/react";
import { Add01Icon, Delete01Icon } from "@hugeicons/core-free-icons";
import { toast } from "sonner";
import { useConfirm } from "@/components/providers/confirm-provider";
import {
  useCreateDashboard,
  useDashboards,
  useDeleteDashboard,
} from "@/hooks/api/use-dashboards";

export function DashboardPanel() {
  const t = useTranslations("workbench.bottomPanel.dashboard");
  const { canWrite } = useAuth();
  const confirmDialog = useConfirm();

  const { data, isLoading, isError } = useDashboards({ limit: 50 });
  const dashboards = data?.items ?? [];

  const createMutation = useCreateDashboard();
  const deleteMutation = useDeleteDashboard();

  if (isError) {
    toast.error(t("loadFailed"));
  }

  const handleCreate = () => {
    createMutation.mutate(
      { name: t("defaultName", { number: dashboards.length + 1 }) },
      {
        onSuccess: () => toast.success(t("created")),
        onError: () => toast.error(t("toast.createFailed")),
      },
    );
  };

  const handleDelete = async (id: string) => {
    const confirmed = await confirmDialog({
      title: t("deleteConfirmTitle"),
      description: t("deleteConfirmDescription"),
      confirmLabel: t("deleteConfirmLabel"),
      variant: "danger",
    });
    if (!confirmed) return;

    deleteMutation.mutate(id, {
      onSuccess: () => toast.success(t("deleted")),
      onError: () => toast.error(t("deleteFailed")),
    });
  };

  if (isLoading) {
    return (
      <div className="flex items-center justify-center py-20">
        <Spinner size="lg" className="text-brand-foreground" />
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col bg-surface-raised p-4">
      <div className="flex items-center justify-between">
        <h2 className="text-sm font-semibold text-foreground">
          {t("title")}
          <span className="ml-2 text-xs font-normal text-muted-foreground">
            {dashboards.length}
          </span>
        </h2>
        {canWrite && (
          <button
            onClick={handleCreate}
            disabled={createMutation.isPending}
            className="flex items-center gap-1 rounded-md bg-brand-solid px-2.5 py-1 text-xs font-medium text-white transition-colors hover:bg-brand-solid disabled:opacity-50"
          >
            <HugeiconsIcon icon={Add01Icon} className="h-3 w-3" size="100%" />
            {t("new")}
          </button>
        )}
      </div>

      {dashboards.length === 0 ? (
        <div className="flex flex-1 items-center justify-center">
          <div className="text-center">
            <p className="text-sm text-muted-foreground">{t("empty")}</p>
            <p className="mt-1 text-xs text-muted-foreground">
              {t("emptyHint")}
            </p>
          </div>
        </div>
      ) : (
        <div className="mt-3 grid gap-2 overflow-y-auto">
          {dashboards.map((d) => (
            <div
              key={d.id}
              className="flex items-center justify-between rounded-lg border border-divider bg-surface-base px-4 py-3"
            >
              <div>
                <h3 className="text-sm font-medium text-foreground-strong">
                  {d.name}
                </h3>
                {d.description && (
                  <p className="mt-0.5 text-xs text-muted-foreground">{d.description}</p>
                )}
                <p className="mt-0.5 text-2xs text-muted-foreground">
                  {t("updated", { date: new Date(d.updated_at).toLocaleDateString() })}
                </p>
              </div>
              {canWrite && (
                <button
                  onClick={() => handleDelete(d.id)}
                  className="rounded p-1 text-muted-foreground transition-colors hover:bg-danger-surface hover:text-danger-foreground dark:hover:bg-danger-surface"
                  aria-label={t("deleteAria")}
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
