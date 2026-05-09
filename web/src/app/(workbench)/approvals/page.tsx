"use client";

import { useMemo, useState } from "react";
import { toast } from "@/components/ui/toast";
import { Heading } from "@/components/ui/heading";
import { useTranslations } from "next-intl";

import { SkeletonCard } from "@/components/ui/skeleton";
import { PageStateView } from "@/components/layout/page-state-view";
import type { PageState } from "@/components/layout/page-state";
import { BulkActionBar } from "@/components/ui/bulk-action-bar";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { DataTable, type ColumnDef } from "@/components/ui/data-table";
import { FieldLabelText, FormTextarea } from "@/components/ui/form-input";
import { StatusBadge, type StatusTone } from "@/components/ui/status-badge";
import { WorkbenchPageShell } from "@/components/workbench/workbench-page-shell";
import { EditOpPreview } from "@/components/settings/approvals/edit-op-preview";
import { CommentThread } from "@/components/settings/approvals/comment-thread";
import {
  useApprovals,
  useBulkReviewApprovals,
  useReviewApproval,
} from "@/hooks/api/use-approvals";
import { usePublishModeCount } from "@/hooks/use-publish-mode-count";
import { useTableUrlState } from "@/hooks/use-table-url-state";
import { cn } from "@/lib/cn";
import type { components } from "@/types/api.generated";

type ApprovalRequest = components["schemas"]["ApprovalRequest"];

type KnownStatus = "pending" | "approved" | "rejected" | "expired";

function isKnownStatus(s: string): s is KnownStatus {
  return s === "pending" || s === "approved" || s === "rejected" || s === "expired";
}

function statusToneFor(status: string): StatusTone {
  switch (status) {
    case "pending":
      return "warning";
    case "approved":
      return "success";
    case "rejected":
      return "danger";
    default:
      return "neutral";
  }
}

function statusLabelFor(
  status: string,
  t: (key: string) => string,
): string {
  return isKnownStatus(status) ? t(`status.${status}`) : status;
}

export default function ApprovalsSettingsPage() {
  const t = useTranslations("settings.governance.approvals");
  const tCommon = useTranslations("common");
  const [expanded, setExpanded] = useState<string | null>(null);
  // Per-row decision-time rationale. Keyed on approval id so a
  // partly-typed note survives a row-toggle. Cleared on successful
  // review.
  const [notes, setNotes] = useState<Record<string, string>>({});
  // Bulk-select cohort. Pending-only — resolved rows are read-only.
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());

  const query = useApprovals();
  const reviewMutation = useReviewApproval();
  const bulkReview = useBulkReviewApprovals();

  const handleReview = (id: string, approved: boolean) => {
    const note = notes[id]?.trim();
    reviewMutation.mutate(
      { id, approved, note: note ? note : undefined },
      {
        onSuccess: () => {
          setNotes((prev) => {
            const next = { ...prev };
            delete next[id];
            return next;
          });
          toast.success(approved ? t("toast.approved") : t("toast.rejected"));
        },
        onError: () => toast.error(t("toast.reviewFailed")),
      },
    );
  };

  const approvals = query.data ?? [];
  // Derived collections + selection memos sit ABOVE the
  // loading/error early-return so the hook order is identical
  // across renders. React's rules-of-hooks: a `useMemo` after a
  // conditional return triggers "rendered more hooks" violations.
  const pending = useMemo(
    () => approvals.filter((a) => a.status === "pending"),
    [approvals],
  );
  usePublishModeCount("approvals", pending.length, "warning");
  const resolved = useMemo(
    () => approvals.filter((a) => a.status !== "pending"),
    [approvals],
  );
  const selectedVisible = useMemo(
    () => pending.filter((a) => selectedIds.has(a.id)),
    [pending, selectedIds],
  );
  const allVisibleSelected =
    pending.length > 0 && selectedVisible.length === pending.length;
  const someVisibleSelected =
    selectedVisible.length > 0 && !allVisibleSelected;

  const url = useTableUrlState();
  const historyColumns = useMemo<
    ColumnDef<ApprovalRequest, unknown>[]
  >(
    () => [
      {
        id: "action",
        header: t("column.action"),
        accessorKey: "action_type",
        cell: ({ getValue }) => (
          <span className="text-foreground-strong">
            {getValue<string>().replace(/_/g, " ")}
          </span>
        ),
      },
      {
        id: "resource",
        header: t("column.resource"),
        accessorFn: (row) => `${row.resource_type} ${row.resource_id}`,
        enableSorting: false,
        cell: ({ row }) => (
          <span className="text-foreground-muted">
            {row.original.resource_type} {row.original.resource_id.slice(0, 8)}
            …
          </span>
        ),
      },
      {
        id: "status",
        header: t("column.status"),
        accessorKey: "status",
        cell: ({ row }) => (
          <StatusBadge tone={statusToneFor(row.original.status)}>
            {statusLabelFor(row.original.status, t)}
          </StatusBadge>
        ),
      },
      {
        id: "created_at",
        header: t("column.date"),
        accessorKey: "created_at",
        cell: ({ getValue }) => (
          <span className="text-foreground-muted">
            {new Date(getValue<string>()).toLocaleDateString()}
          </span>
        ),
      },
    ],
    [t],
  );

  if (query.isLoading || query.isError) {
    const pageState: PageState = query.isLoading
      ? { kind: "loading" }
      : { kind: "error", onRetry: () => void query.refetch() };
    return (
      <WorkbenchPageShell title={t("title")} pageState={pageState}>
        <PageStateView
          state={pageState}
          skeleton={
            <div className="space-y-3">
              <SkeletonCard />
              <SkeletonCard />
            </div>
          }
          error={{
            title: tCommon("loadError.title"),
            description: tCommon("loadError.description"),
            retryLabel: tCommon("retry"),
          }}
        >
          <></>
        </PageStateView>
      </WorkbenchPageShell>
    );
  }
  const toggleSelected = (id: string) =>
    setSelectedIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  const clearSelection = () => setSelectedIds(new Set());
  const toggleSelectAll = () => {
    if (allVisibleSelected) {
      clearSelection();
    } else {
      setSelectedIds(new Set(pending.map((a) => a.id)));
    }
  };

  const handleBulkReview = (approved: boolean) => {
    const ids = Array.from(selectedIds);
    if (ids.length === 0) return;
    bulkReview.mutate(
      { ids, approved },
      {
        onSuccess: ({ reviewed }) => {
          clearSelection();
          toast.success(
            t(approved ? "toast.bulkApproved" : "toast.bulkRejected", {
              count: reviewed,
            }),
          );
        },
        onError: () => toast.error(t("toast.bulkFailed")),
      },
    );
  };

  return (
    <WorkbenchPageShell title={t("title")} count={pending.length}>
      {pending.length > 0 && (
        <div>
          <div className="flex items-center justify-between">
            <Heading level={2} size={6}>
              {t("pendingHeading", { count: pending.length })}
            </Heading>
            <div className="flex items-center gap-2 text-xs text-foreground-muted">
              <Checkbox
                checked={allVisibleSelected}
                indeterminate={someVisibleSelected}
                onChange={toggleSelectAll}
                aria-label={t("selectAll")}
              />
              <span>{t("selectAllHint", { count: pending.length })}</span>
            </div>
          </div>
          <div className="mt-2 space-y-3">
            {pending.map((a) => {
              const isOpen = expanded === a.id;
              const isSelected = selectedIds.has(a.id);
              return (
                <div
                  key={a.id}
                  className={cn(
                    "rounded-lg border border-warning-border bg-warning-surface",
                    isSelected && "ring-2 ring-brand-foreground/40",
                  )}
                >
                  <div className="flex items-center justify-between p-4">
                    <div
                      // Checkbox sits OUTSIDE the toggle button so a
                      // click on it never triggers expand/collapse.
                      className="me-3 flex shrink-0"
                      onClick={(e) => e.stopPropagation()}
                    >
                      <Checkbox
                        checked={isSelected}
                        onChange={() => toggleSelected(a.id)}
                        aria-label={t("selectRow", { action: a.action_type })}
                      />
                    </div>
                    <button
                      type="button"
                      onClick={() => setExpanded(isOpen ? null : a.id)}
                      className="flex-1 text-start"
                      aria-expanded={isOpen}
                    >
                      <div className="font-medium text-foreground-strong">
                        {a.action_type.replace(/_/g, " ")}
                      </div>
                      <div className="text-xs text-foreground-muted">
                        {a.resource_type} {a.resource_id.slice(0, 8)}... |{" "}
                        {t("expires")}{" "}
                        {new Date(a.expires_at).toLocaleDateString()} ·{" "}
                        {isOpen ? t("collapsePreview") : t("expandPreview")}
                      </div>
                    </button>
                    <div className="flex gap-2">
                      <Button
                        variant="primary"
                        size="sm"
                        onClick={() => handleReview(a.id, true)}
                      >
                        {t("approve")}
                      </Button>
                      <Button
                        variant="danger"
                        size="sm"
                        onClick={() => handleReview(a.id, false)}
                      >
                        {t("reject")}
                      </Button>
                    </div>
                  </div>
                  {isOpen && (
                    <div className="flex flex-col gap-3 border-t border-warning-border bg-surface-base px-4 pb-4 pt-3">
                      <EditOpPreview payload={a.payload} />
                      <div className="flex flex-col gap-1.5">
                        <label htmlFor={`approval-note-${a.id}`}>
                          <FieldLabelText label={t("reviewerNote.label")} />
                        </label>
                        <FormTextarea
                          id={`approval-note-${a.id}`}
                          value={notes[a.id] ?? ""}
                          onChange={(e) =>
                            setNotes((prev) => ({
                              ...prev,
                              [a.id]: e.target.value,
                            }))
                          }
                          placeholder={t("reviewerNote.placeholder")}
                          rows={2}
                          density="settings"
                        />
                      </div>
                      <CommentThread approvalId={a.id} />
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        </div>
      )}

      {/* History */}
      <div className="mt-6">
        <Heading level={2} size={6}>
          {t("historyHeading")}
        </Heading>
        <div className="mt-2">
          <DataTable<ApprovalRequest>
            columns={historyColumns}
            data={resolved}
            rowId={(row) => row.id}
            sort={url.sort}
            onSortChange={url.setSort}
            onRowClick={(row) =>
              setExpanded(expanded === row.id ? null : row.id)
            }
            expandedRowId={expanded}
            renderRowExpansion={(row) => (
              <CommentThread approvalId={row.id} readOnly />
            )}
            ariaLabel={t("historyHeading")}
            emptyState={
              <p className="py-8 text-center text-foreground-muted">
                {t("emptyHistory")}
              </p>
            }
          />
        </div>
      </div>

      <BulkActionBar
        count={selectedIds.size}
        countLabel={t("bulkSelectedCount", { count: selectedIds.size })}
        clearLabel={t("bulkClear")}
        ariaLabel={t("bulkBarLabel")}
        actions={[
          {
            key: "reject",
            label: t("bulkReject"),
            variant: "danger",
            onClick: () => handleBulkReview(false),
          },
          {
            key: "approve",
            label: t("bulkApprove"),
            variant: "primary",
            onClick: () => handleBulkReview(true),
          },
        ]}
        onClear={clearSelection}
        pending={bulkReview.isPending}
      />
    </WorkbenchPageShell>
  );
}
