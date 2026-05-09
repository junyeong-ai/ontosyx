"use client";

import { useMemo, useState } from "react";
import { useTranslations } from "next-intl";
import Link from "next/link";
import { Settings2 } from "lucide-react";

import { useAuth } from "@/hooks/use-auth";
import {
  useCancelEvaluationRun,
  useDeleteEvaluationRun,
  useEvaluationRuns,
} from "@/hooks/api/use-evaluation";
import { usePublishModeCount } from "@/hooks/use-publish-mode-count";
import { useTableUrlState } from "@/hooks/use-table-url-state";
import { useConfirm } from "@/components/providers/confirm-provider";
import { RegressionPolicyForm } from "@/components/settings/evaluation/regression-policy-form";
import { WorkbenchPageShell } from "@/components/workbench/workbench-page-shell";
import { PageStateView } from "@/components/layout/page-state-view";
import type { PageState } from "@/components/layout/page-state";
import { Button, buttonStyles } from "@/components/ui/button";
import { DataTable, type ColumnDef } from "@/components/ui/data-table";
import { EmptyState } from "@/components/ui/empty-state";
import { Modal } from "@/components/ui/modal";
import { SkeletonList } from "@/components/ui/skeleton";
import { Heading } from "@/components/ui/heading";
import { StatusBadge, type StatusTone } from "@/components/ui/status-badge";
import { toast } from "@/components/ui/toast";
import type { EvaluationRun, EvaluationRunStatus } from "@/types/evaluation";

const STATUS_TONE: Record<EvaluationRunStatus, StatusTone> = {
  running: "info",
  succeeded: "success",
  failed: "danger",
  cancelled: "neutral",
};

function formatTimestamp(value: string) {
  const d = new Date(value);
  if (Number.isNaN(d.getTime())) return value;
  return d.toLocaleString();
}

export default function EvaluationPage() {
  const t = useTranslations("settings.evaluation");
  const tPolicy = useTranslations("settings.evaluation.regressionPolicy");
  const tCommon = useTranslations("common");
  const { isAdmin } = useAuth();
  const query = useEvaluationRuns();
  const cancel = useCancelEvaluationRun();
  const remove = useDeleteEvaluationRun();
  const confirm = useConfirm();
  const url = useTableUrlState();
  const [policyOpen, setPolicyOpen] = useState(false);

  const onCancel = (id: string) => {
    cancel.mutate(id, {
      onSuccess: () => toast.success(t("table.cancelSuccessToast")),
      onError: (err) =>
        toast.error(
          t("table.cancelErrorToast", {
            error: err instanceof Error ? err.message : String(err),
          }),
        ),
    });
  };
  const onDelete = async (id: string, name: string) => {
    const ok = await confirm({
      title: t("table.delete"),
      description: t("table.deleteConfirm", { name }),
      variant: "danger",
    });
    if (!ok) return;
    remove.mutate(id, {
      onSuccess: () => toast.success(t("table.deleteSuccessToast")),
      onError: (err) =>
        toast.error(
          t("table.deleteErrorToast", {
            error: err instanceof Error ? err.message : String(err),
          }),
        ),
    });
  };

  const runs = query.data?.items ?? [];
  const failedCount = runs.filter((r) => r.status === "failed").length;
  usePublishModeCount("evaluation", failedCount, "danger");

  if (!isAdmin) {
    return (
      <WorkbenchPageShell title={t("title")}>
        <EmptyState title={t("adminOnly")} />
      </WorkbenchPageShell>
    );
  }

  const pageState: PageState = query.isLoading
    ? { kind: "loading" }
    : query.isError
      ? { kind: "error", onRetry: () => void query.refetch() }
      : runs.length === 0
        ? { kind: "empty" }
        : { kind: "data" };

  return (
    <WorkbenchPageShell
      title={t("title")}
      count={runs.length}
      pageState={pageState}
      actions={
        <>
          <Button
            type="button"
            variant="ghost"
            size="icon-sm"
            onClick={() => setPolicyOpen(true)}
            tooltip={tPolicy("title")}
            aria-label={tPolicy("title")}
          >
            <Settings2 className="h-3.5 w-3.5" aria-hidden />
          </Button>
          <Link
            href="/evaluation/datasets"
            className={buttonStyles({ variant: "outline", size: "sm" })}
          >
            {t("create.openDatasets")}
          </Link>
        </>
      }
    >
      <Modal
        open={policyOpen}
        onOpenChange={setPolicyOpen}
        title={tPolicy("title")}
        size="lg"
      >
        <RegressionPolicyForm />
      </Modal>
      <PageStateView
        state={pageState}
        skeleton={<SkeletonList count={4} />}
        error={{
          title: tCommon("loadError.title"),
          description: tCommon("loadError.description"),
          retryLabel: tCommon("retry"),
        }}
        empty={{
          title: t("emptyTitle"),
          description: t("emptyDescription"),
        }}
      >
        <RunsTable
          runs={runs}
          sort={url.sort}
          onSortChange={url.setSort}
          onCancel={onCancel}
          onDelete={onDelete}
          cancelPending={cancel.isPending}
          removePending={remove.isPending}
        />
      </PageStateView>
    </WorkbenchPageShell>
  );
}

function RunsTable({
  runs,
  sort,
  onSortChange,
  onCancel,
  onDelete,
  cancelPending,
  removePending,
}: {
  runs: EvaluationRun[];
  sort: ReturnType<typeof useTableUrlState>["sort"];
  onSortChange: ReturnType<typeof useTableUrlState>["setSort"];
  onCancel: (id: string) => void;
  onDelete: (id: string, name: string) => void;
  cancelPending: boolean;
  removePending: boolean;
}) {
  const t = useTranslations("settings.evaluation");
  const columns = useMemo<ColumnDef<EvaluationRun, unknown>[]>(
    () => [
      {
        id: "name",
        header: t("table.name"),
        accessorKey: "name",
        cell: ({ row }) => (
          <Link
            href={`/evaluation/${row.original.id}`}
            className="block hover:underline"
          >
            <Heading level={3} size={6}>
              {row.original.name}
            </Heading>
            {row.original.description ? (
              <p className="mt-0.5 text-2xs text-foreground-muted">
                {row.original.description}
              </p>
            ) : null}
          </Link>
        ),
      },
      {
        id: "status",
        header: t("table.status"),
        accessorKey: "status",
        cell: ({ row }) => (
          <StatusBadge tone={STATUS_TONE[row.original.status]}>
            {t(`status.${row.original.status}`)}
          </StatusBadge>
        ),
      },
      {
        id: "origin",
        header: t("table.origin"),
        enableSorting: false,
        cell: ({ row }) =>
          row.original.fingerprint?.dataset_id ? (
            <Link
              href={`/evaluation/datasets/${encodeURIComponent(row.original.fingerprint.dataset_id)}`}
              className="inline-flex items-center gap-1 rounded-full bg-info-surface px-2 py-0.5 text-2xs text-info-foreground ring-1 ring-info-border hover:underline"
              onClick={(e) => e.stopPropagation()}
            >
              {t("table.fromDataset")}
            </Link>
          ) : (
            <span className="inline-flex items-center rounded-full bg-surface-inset px-2 py-0.5 text-2xs text-foreground-muted ring-1 ring-divider">
              {t("table.adHoc")}
            </span>
          ),
      },
      {
        id: "started_at",
        header: t("table.startedAt"),
        accessorKey: "started_at",
        cell: ({ getValue }) => (
          <span className="tabular-nums text-foreground-muted">
            {formatTimestamp(getValue<string>())}
          </span>
        ),
      },
      {
        id: "completed_at",
        header: t("table.completedAt"),
        accessorKey: "completed_at",
        cell: ({ getValue }) => {
          const v = getValue<string | null | undefined>();
          return (
            <span className="tabular-nums text-foreground-muted">
              {v ? formatTimestamp(v) : "—"}
            </span>
          );
        },
      },
      {
        id: "actions",
        header: t("table.actions"),
        enableSorting: false,
        meta: { headerClass: "text-end", cellClass: "text-end" },
        cell: ({ row }) => (
          <div className="flex justify-end gap-2">
            {row.original.status === "running" ? (
              <Button
                size="sm"
                variant="outline"
                onClick={() => onCancel(row.original.id)}
                disabled={cancelPending}
                title={t("table.cancelTitle")}
              >
                {t("table.cancel")}
              </Button>
            ) : null}
            <Button
              size="sm"
              variant="danger"
              onClick={() => onDelete(row.original.id, row.original.name)}
              disabled={removePending}
            >
              {t("table.delete")}
            </Button>
          </div>
        ),
      },
    ],
    [t, cancelPending, removePending, onCancel, onDelete],
  );

  return (
    <DataTable<EvaluationRun>
      columns={columns}
      data={runs}
      rowId={(row) => row.id}
      sort={sort}
      onSortChange={onSortChange}
      ariaLabel={t("title")}
    />
  );
}
