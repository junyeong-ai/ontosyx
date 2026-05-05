"use client";

import { useTranslations } from "next-intl";
import { useAuth } from "@/hooks/use-auth";
import { EmptyState } from "@/components/ui/empty-state";
import { Heading } from "@/components/ui/heading";
import { ErrorState } from "@/components/ui/error-state";
import { SkeletonList } from "@/components/ui/skeleton";
import { LayoutDashboard, Plus, Trash2 } from "lucide-react";
import { toast } from "@/components/ui/toast";
import { useConfirm } from "@/components/providers/confirm-provider";
import {
  useCreateDashboard,
  useDashboards,
  useDeleteDashboard,
} from "@/hooks/api/use-dashboards";

export function DashboardPanel() {
  const t = useTranslations("workbench.bottomPanel.dashboard");
  const tCommon = useTranslations("common");
  const { canWrite } = useAuth();
  const confirmDialog = useConfirm();

  const dashboardsQuery = useDashboards({ limit: 50 });
  const { data, isLoading, isError } = dashboardsQuery;
  const dashboards = data?.items ?? [];

  const createMutation = useCreateDashboard();
  const deleteMutation = useDeleteDashboard();

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

  return (
    <div className="flex h-full flex-col bg-surface-raised p-4">
      <div className="flex items-center justify-between">
        <Heading level={2} size={6}>
          {t("title")}
          <span className="ms-2 text-xs font-normal text-foreground-muted">
            {dashboards.length}
          </span>
        </Heading>
        {canWrite && (
          <button type="button"
            onClick={handleCreate}
            disabled={createMutation.isPending || isLoading || isError}
            className="flex items-center gap-1 rounded-md bg-brand-solid px-2.5 py-1 text-xs font-medium text-foreground-onbrand transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] hover:bg-brand-solid disabled:opacity-50"
          >
            <Plus className="h-3 w-3" />
            {t("new")}
          </button>
        )}
      </div>

      {isLoading ? (
        <div className="mt-3">
          <SkeletonList count={3} />
        </div>
      ) : isError ? (
        <div className="flex flex-1 items-center">
          <ErrorState
            title={tCommon("loadError.title")}
            description={t("loadFailed")}
            onRetry={() => dashboardsQuery.refetch()}
            retryLabel={tCommon("retry")}
          />
        </div>
      ) : dashboards.length === 0 ? (
        <div className="flex flex-1 items-center">
          <EmptyState
            icon={LayoutDashboard}
            title={t("empty")}
            description={t("emptyHint")}
            variant="compact"
          />
        </div>
      ) : (
        <div className="mt-3 grid gap-2 overflow-y-auto">
          {dashboards.map((d) => (
            <div
              key={d.id}
              className="flex items-center justify-between rounded-lg border border-divider bg-surface-base px-4 py-3"
            >
              <div>
                <Heading level={3} size={6} className="font-medium">
                  {d.name}
                </Heading>
                {d.description && (
                  <p className="mt-0.5 text-xs text-foreground-muted">{d.description}</p>
                )}
                <p className="mt-0.5 text-2xs text-foreground-muted">
                  {t("updated", { date: new Date(d.updated_at).toLocaleDateString() })}
                </p>
              </div>
              {canWrite && (
                <button type="button"
                  onClick={() => handleDelete(d.id)}
                  className="rounded p-1 text-foreground-muted transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] hover:bg-danger-surface hover:text-danger-foreground"
                  aria-label={t("deleteAria")}
                >
                  <Trash2 className="h-3.5 w-3.5" />
                </button>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
