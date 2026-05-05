"use client";

import { Fragment, useMemo, useState } from "react";
import { toast } from "@/components/ui/toast";
import { Heading } from "@/components/ui/heading";
import { useTranslations } from "next-intl";

import { SkeletonCard } from "@/components/ui/skeleton";
import { PageStateView } from "@/components/layout/page-state-view";
import type { PageState } from "@/components/layout/page-state";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { FormTextarea } from "@/components/ui/form-input";
import { SettingsPageShell } from "@/components/layout/settings-page-shell";
import { EditOpPreview } from "@/components/settings/approvals/edit-op-preview";
import { CommentThread } from "@/components/settings/approvals/comment-thread";
import {
  useApprovals,
  useBulkReviewApprovals,
  useReviewApproval,
} from "@/hooks/api/use-approvals";
import { cn } from "@/lib/cn";

type KnownStatus = "pending" | "approved" | "rejected" | "expired";

function isKnownStatus(s: string): s is KnownStatus {
  return s === "pending" || s === "approved" || s === "rejected" || s === "expired";
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

  if (query.isLoading || query.isError) {
    const pageState: PageState = query.isLoading
      ? { kind: "loading" }
      : { kind: "error", onRetry: () => void query.refetch() };
    return (
      <SettingsPageShell title={t("title")} subtitle={t("subtitle")}>
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
      </SettingsPageShell>
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

  const statusBadge = (status: string) => {
    switch (status) {
      case "pending": return "bg-warning-surface text-warning-foreground";
      case "approved": return "bg-success-surface text-success-foreground";
      case "rejected": return "bg-danger-surface text-danger-foreground";
      case "expired": return "bg-surface-inset text-foreground-muted";
      default: return "bg-surface-inset text-foreground-muted";
    }
  };

  // Translated status label; unknown status falls back to the raw string
  // so the UI still surfaces *something* rather than a blank pill.
  const statusLabel = (status: string) =>
    isKnownStatus(status) ? t(`status.${status}`) : status;

  return (
    <SettingsPageShell title={t("title")} subtitle={t("description")}>
      {pending.length > 0 && (
        <div>
          <div className="flex items-center justify-between">
            <h2 className="text-sm font-semibold text-foreground">
              {t("pendingHeading", { count: pending.length })}
            </h2>
            <div className="flex items-center gap-2 text-xs text-foreground-muted">
              <Checkbox
                checked={allVisibleSelected}
                ref={(el) => {
                  if (el) el.indeterminate = someVisibleSelected;
                }}
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
                        <label
                          htmlFor={`approval-note-${a.id}`}
                          className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted"
                        >
                          {t("reviewerNote.label")}
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
        <table className="mt-2 w-full text-sm">
          <thead>
            <tr className="border-b border-divider text-start text-xs font-medium uppercase text-foreground-muted">
              <th className="py-3 pe-6">{t("column.action")}</th>
              <th className="py-3 pe-6">{t("column.resource")}</th>
              <th className="py-3 pe-6">{t("column.status")}</th>
              <th className="py-3 pe-6">{t("column.date")}</th>
            </tr>
          </thead>
          <tbody>
            {resolved.map((a) => {
              const isOpen = expanded === a.id;
              const toggle = () => setExpanded(isOpen ? null : a.id);
              return (
                <Fragment key={a.id}>
                  {/* Row is expandable — `role="button"` + tabIndex
                      + Enter/Space handler so keyboard users can
                      toggle. `aria-expanded` advertises state to AT.
                      A real <button type="button"> can't wrap a <tr>, so we lift
                      the keyboard semantics onto the row itself. */}
                  <tr
                    role="button"
                    tabIndex={0}
                    aria-expanded={isOpen}
                    onClick={toggle}
                    onKeyDown={(e) => {
                      if (e.key === "Enter" || e.key === " ") {
                        e.preventDefault();
                        toggle();
                      }
                    }}
                    className="cursor-pointer border-b border-divider-soft hover:bg-surface-raised focus:bg-surface-raised focus:outline-none focus-visible:ring-2 focus-visible:ring-brand-foreground/60"
                  >
                    <td className="py-3 pe-6 text-foreground-strong">
                      {a.action_type.replace(/_/g, " ")}
                    </td>
                    <td className="py-3 pe-6 text-foreground-muted">
                      {a.resource_type} {a.resource_id.slice(0, 8)}...
                    </td>
                    <td className="py-3 pe-6">
                      <span className={`rounded-full px-2 py-0.5 text-xs font-medium ${statusBadge(a.status)}`}>
                        {statusLabel(a.status)}
                      </span>
                    </td>
                    <td className="py-3 pe-6 text-foreground-muted">
                      {new Date(a.created_at).toLocaleDateString()}
                    </td>
                  </tr>
                  {isOpen && (
                    <tr className="border-b border-divider-soft">
                      <td colSpan={4} className="bg-surface-raised px-4 py-3">
                        <CommentThread approvalId={a.id} readOnly />
                      </td>
                    </tr>
                  )}
                </Fragment>
              );
            })}
            {approvals.length === 0 && (
              <tr>
                <td colSpan={4} className="py-8 text-center text-foreground-muted">
                  {t("emptyHistory")}
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>

      <BulkActionBar
        count={selectedIds.size}
        onApprove={() => handleBulkReview(true)}
        onReject={() => handleBulkReview(false)}
        onClear={clearSelection}
        pending={bulkReview.isPending}
      />
    </SettingsPageShell>
  );
}

function BulkActionBar({
  count,
  onApprove,
  onReject,
  onClear,
  pending,
}: {
  count: number;
  onApprove: () => void;
  onReject: () => void;
  onClear: () => void;
  pending: boolean;
}) {
  const t = useTranslations("settings.governance.approvals");
  const visible = count > 0;
  return (
    <div
      className={cn(
        "pointer-events-none fixed inset-x-0 bottom-6 z-30 mx-auto flex max-w-2xl",
        "items-center justify-between gap-3 rounded-xl border border-divider",
        "bg-surface-overlay px-4 py-3 shadow-2",
        "transition-all duration-[var(--duration-base)] ease-[var(--ease-out)]",
        visible ? "translate-y-0 opacity-100" : "translate-y-4 opacity-0",
      )}
      role="region"
      aria-label={t("bulkBarLabel")}
      aria-hidden={!visible}
    >
      <span className="text-sm font-medium text-foreground-strong">
        {t("bulkSelectedCount", { count })}
      </span>
      <div className="flex items-center gap-2">
        <Button
          variant="outline"
          size="sm"
          onClick={onClear}
          disabled={pending}
          className="pointer-events-auto"
        >
          {t("bulkClear")}
        </Button>
        <Button
          variant="danger"
          size="sm"
          onClick={onReject}
          disabled={pending}
          className="pointer-events-auto"
        >
          {t("bulkReject")}
        </Button>
        <Button
          variant="primary"
          size="sm"
          onClick={onApprove}
          disabled={pending}
          className="pointer-events-auto"
        >
          {t("bulkApprove")}
        </Button>
      </div>
    </div>
  );
}
