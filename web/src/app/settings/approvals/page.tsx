"use client";

import { Fragment, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { useTranslations } from "next-intl";

import { request } from "@/lib/api/client";
import { ErrorState } from "@/components/ui/error-state";
import { SkeletonCard } from "@/components/ui/skeleton";
import { SettingsPageShell } from "@/components/layout/settings-page-shell";
import { EditOpPreview } from "@/components/settings/approvals/edit-op-preview";
import { CommentThread } from "@/components/settings/approvals/comment-thread";
import type { components } from "@/types/api.generated";

type ApprovalRequest = components["schemas"]["ApprovalRequest"];

type KnownStatus = "pending" | "approved" | "rejected" | "expired";

function isKnownStatus(s: string): s is KnownStatus {
  return s === "pending" || s === "approved" || s === "rejected" || s === "expired";
}

const approvalsKeys = {
  all: ["approvals"] as const,
  list: () => [...approvalsKeys.all, "list"] as const,
};

export default function ApprovalsSettingsPage() {
  const t = useTranslations("settings.approvals");
  const tCommon = useTranslations("common");
  const qc = useQueryClient();
  const [expanded, setExpanded] = useState<string | null>(null);
  // Per-row decision-time rationale. Keyed on approval id so a
  // partly-typed note survives a row-toggle. Cleared on successful
  // review.
  const [notes, setNotes] = useState<Record<string, string>>({});

  const query = useQuery({
    queryKey: approvalsKeys.list(),
    queryFn: async () => {
      const data = await request<ApprovalRequest[]>("/approvals");
      return Array.isArray(data) ? data : [];
    },
  });

  const reviewMutation = useMutation({
    mutationFn: ({
      id,
      approved,
      note,
    }: {
      id: string;
      approved: boolean;
      note: string | undefined;
    }) =>
      request(`/approvals/${id}/review`, {
        method: "POST",
        body: JSON.stringify({ approved, note }),
      }),
    onSuccess: (_data, { id, approved }) => {
      setNotes((prev) => {
        const next = { ...prev };
        delete next[id];
        return next;
      });
      toast.success(approved ? t("toast.approved") : t("toast.rejected"));
      qc.invalidateQueries({ queryKey: approvalsKeys.list() });
    },
    onError: () => toast.error(t("toast.reviewFailed")),
  });

  const handleReview = (id: string, approved: boolean) => {
    const note = notes[id]?.trim();
    reviewMutation.mutate({ id, approved, note: note ? note : undefined });
  };

  if (query.isLoading) {
    return (
      <SettingsPageShell title={t("title")} subtitle={t("subtitle")}>
        <div className="space-y-3">
          <SkeletonCard />
          <SkeletonCard />
        </div>
      </SettingsPageShell>
    );
  }

  if (query.isError) {
    return (
      <SettingsPageShell title={t("title")} subtitle={t("subtitle")}>
        <ErrorState
          title={tCommon("loadError.title")}
          description={tCommon("loadError.description")}
          onRetry={() => query.refetch()}
          retryLabel={tCommon("retry")}
        />
      </SettingsPageShell>
    );
  }

  const approvals = query.data ?? [];
  const pending = approvals.filter((a) => a.status === "pending");
  const resolved = approvals.filter((a) => a.status !== "pending");

  const statusBadge = (status: string) => {
    switch (status) {
      case "pending": return "bg-warning-surface text-warning-foreground";
      case "approved": return "bg-success-surface text-success-foreground";
      case "rejected": return "bg-danger-surface text-danger-foreground";
      case "expired": return "bg-surface-inset text-foreground-muted";
      default: return "bg-surface-inset text-muted-foreground";
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
          <h2 className="text-sm font-semibold text-foreground">
            {t("pendingHeading", { count: pending.length })}
          </h2>
          <div className="mt-2 space-y-3">
            {pending.map((a) => {
              const isOpen = expanded === a.id;
              return (
                <div
                  key={a.id}
                  className="rounded-lg border border-warning-border bg-warning-surface"
                >
                  <div className="flex items-center justify-between p-4">
                    <button
                      type="button"
                      onClick={() => setExpanded(isOpen ? null : a.id)}
                      className="flex-1 text-left"
                      aria-expanded={isOpen}
                    >
                      <div className="font-medium text-foreground-strong">
                        {a.action_type.replace(/_/g, " ")}
                      </div>
                      <div className="text-xs text-muted-foreground">
                        {a.resource_type} {a.resource_id.slice(0, 8)}... |{" "}
                        {t("expires")}{" "}
                        {new Date(a.expires_at).toLocaleDateString()} ·{" "}
                        {isOpen ? t("collapsePreview") : t("expandPreview")}
                      </div>
                    </button>
                    <div className="flex gap-2">
                      <button
                        onClick={() => handleReview(a.id, true)}
                        className="rounded-md bg-brand-solid px-3 py-1.5 text-xs font-medium text-white hover:bg-brand-solid-hover"
                      >
                        {t("approve")}
                      </button>
                      <button
                        onClick={() => handleReview(a.id, false)}
                        className="rounded-md bg-danger-solid px-3 py-1.5 text-xs font-medium text-white hover:bg-danger-solid-hover"
                      >
                        {t("reject")}
                      </button>
                    </div>
                  </div>
                  {isOpen && (
                    <div className="flex flex-col gap-3 border-t border-warning-border bg-surface-base px-4 pb-4 pt-3">
                      <EditOpPreview payload={a.payload} />
                      <label className="flex flex-col gap-1.5">
                        <span className="text-2xs font-semibold uppercase tracking-wider text-muted-foreground">
                          {t("reviewerNote.label")}
                        </span>
                        <textarea
                          value={notes[a.id] ?? ""}
                          onChange={(e) =>
                            setNotes((prev) => ({
                              ...prev,
                              [a.id]: e.target.value,
                            }))
                          }
                          placeholder={t("reviewerNote.placeholder")}
                          rows={2}
                          className="rounded-md border border-divider bg-surface-base px-3 py-1.5 text-xs transition-colors focus:border-brand-foreground focus:outline-none focus:ring-1 focus:ring-brand-foreground/50 dark:focus:border-brand-border"
                        />
                      </label>
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
        <h2 className="text-sm font-semibold text-foreground">
          {t("historyHeading")}
        </h2>
        <table className="mt-2 w-full text-sm">
          <thead>
            <tr className="border-b border-divider text-left text-xs font-medium uppercase text-muted-foreground">
              <th className="py-3 pr-6">{t("column.action")}</th>
              <th className="py-3 pr-6">{t("column.resource")}</th>
              <th className="py-3 pr-6">{t("column.status")}</th>
              <th className="py-3 pr-6">{t("column.date")}</th>
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
                      A real <button> can't wrap a <tr>, so we lift
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
                    className="cursor-pointer border-b border-divider-soft hover:bg-surface-raised focus:bg-surface-raised focus:outline-none focus-visible:ring-2 focus-visible:ring-brand-foreground/60 dark:hover:bg-surface-base dark:focus:bg-surface-base"
                  >
                    <td className="py-3 pr-6 text-foreground-strong">
                      {a.action_type.replace(/_/g, " ")}
                    </td>
                    <td className="py-3 pr-6 text-muted-foreground">
                      {a.resource_type} {a.resource_id.slice(0, 8)}...
                    </td>
                    <td className="py-3 pr-6">
                      <span className={`rounded-full px-2 py-0.5 text-xs font-medium ${statusBadge(a.status)}`}>
                        {statusLabel(a.status)}
                      </span>
                    </td>
                    <td className="py-3 pr-6 text-muted-foreground">
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
                <td colSpan={4} className="py-8 text-center text-muted-foreground">
                  {t("emptyHistory")}
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>
    </SettingsPageShell>
  );
}
