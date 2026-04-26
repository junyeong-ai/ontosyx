"use client";

import { useState, useEffect, useCallback } from "react";
import { toast } from "sonner";
import { useTranslations } from "next-intl";
import { request } from "@/lib/api/client";
import { Spinner } from "@/components/ui/spinner";
import { EditOpPreview } from "@/components/settings/approvals/edit-op-preview";

interface ApprovalRequest {
  id: string;
  requester_id: string;
  action_type: string;
  resource_type: string;
  resource_id: string;
  /** Action-specific payload — for OntologyEditOp batches this
   *  carries the `OntologyEditRequest` shape (operations +
   *  expected_version + optional message). */
  payload: unknown;
  status: string;
  reviewer_id: string | null;
  review_notes: string | null;
  expires_at: string;
  created_at: string;
}

type KnownStatus = "pending" | "approved" | "rejected" | "expired";

function isKnownStatus(s: string): s is KnownStatus {
  return s === "pending" || s === "approved" || s === "rejected" || s === "expired";
}

export default function ApprovalsSettingsPage() {
  const t = useTranslations("settings.approvals");
  const [approvals, setApprovals] = useState<ApprovalRequest[]>([]);
  const [loading, setLoading] = useState(true);
  const [expanded, setExpanded] = useState<string | null>(null);
  // Per-row reviewer note. Keyed on approval id so a partly-typed
  // note survives a row-toggle. Cleared on successful review.
  const [notes, setNotes] = useState<Record<string, string>>({});

  const load = useCallback(async () => {
    try {
      const data = await request<ApprovalRequest[]>("/approvals");
      setApprovals(Array.isArray(data) ? data : []);
    } catch {
      toast.error(t("toast.loadFailed"));
    } finally {
      setLoading(false);
    }
  }, [t]);

  useEffect(() => { load(); }, [load]);

  const handleReview = async (id: string, approved: boolean) => {
    try {
      const note = notes[id]?.trim();
      await request(`/approvals/${id}/review`, {
        method: "POST",
        body: JSON.stringify({
          approved,
          notes: note ? note : null,
        }),
      });
      // Clear the note now that it landed on the row.
      setNotes((prev) => {
        const next = { ...prev };
        delete next[id];
        return next;
      });
      toast.success(approved ? t("toast.approved") : t("toast.rejected"));
      load();
    } catch {
      toast.error(t("toast.reviewFailed"));
    }
  };

  if (loading) return <Spinner />;

  const pending = approvals.filter((a) => a.status === "pending");
  const resolved = approvals.filter((a) => a.status !== "pending");

  const statusBadge = (status: string) => {
    switch (status) {
      case "pending": return "bg-amber-100 text-amber-700 dark:bg-amber-900/30 dark:text-amber-400";
      case "approved": return "bg-emerald-100 text-emerald-700 dark:bg-emerald-900/30 dark:text-emerald-400";
      case "rejected": return "bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400";
      case "expired": return "bg-zinc-100 text-zinc-500 dark:bg-zinc-800 dark:text-muted-foreground";
      default: return "bg-zinc-100 text-muted-foreground";
    }
  };

  // Translated status label; unknown status falls back to the raw string
  // so the UI still surfaces *something* rather than a blank pill.
  const statusLabel = (status: string) =>
    isKnownStatus(status) ? t(`status.${status}`) : status;

  return (
    <div>
      <h1 className="text-xl font-semibold text-zinc-900 dark:text-zinc-100">
        {t("title")}
      </h1>
      <p className="mt-1 text-sm text-zinc-500 dark:text-muted-foreground">
        {t("description")}
      </p>

      {/* Pending approvals */}
      {pending.length > 0 && (
        <div className="mt-6">
          <h2 className="text-sm font-semibold text-zinc-700 dark:text-zinc-300">
            {t("pendingHeading", { count: pending.length })}
          </h2>
          <div className="mt-2 space-y-3">
            {pending.map((a) => {
              const isOpen = expanded === a.id;
              return (
                <div
                  key={a.id}
                  className="rounded-lg border border-amber-200 bg-amber-50 dark:border-amber-900 dark:bg-amber-950"
                >
                  <div className="flex items-center justify-between p-4">
                    <button
                      type="button"
                      onClick={() => setExpanded(isOpen ? null : a.id)}
                      className="flex-1 text-left"
                      aria-expanded={isOpen}
                    >
                      <div className="font-medium text-zinc-900 dark:text-zinc-100">
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
                        className="rounded-md bg-emerald-700 px-3 py-1.5 text-xs font-medium text-white hover:bg-emerald-800"
                      >
                        {t("approve")}
                      </button>
                      <button
                        onClick={() => handleReview(a.id, false)}
                        className="rounded-md bg-red-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-red-700"
                      >
                        {t("reject")}
                      </button>
                    </div>
                  </div>
                  {isOpen && (
                    <div className="flex flex-col gap-3 border-t border-amber-200 bg-white px-4 pb-4 pt-3 dark:border-amber-900 dark:bg-zinc-900">
                      <EditOpPreview payload={a.payload} />
                      {/* Φ6 #2 (interim) — reviewer note. The
                          backend already accepts `notes` on
                          /review; until the proper comment-thread
                          table lands (migration 0030), this single
                          field captures the reviewer's rationale on
                          decision. */}
                      <label className="flex flex-col gap-1.5">
                        <span className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
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
                          className="rounded-md border border-zinc-300 bg-white px-3 py-1.5 text-xs transition-colors focus:border-emerald-500 focus:outline-none focus:ring-1 focus:ring-emerald-500/50 dark:border-zinc-700 dark:bg-zinc-900 dark:focus:border-emerald-400"
                        />
                      </label>
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
        <h2 className="text-sm font-semibold text-zinc-700 dark:text-zinc-300">
          {t("historyHeading")}
        </h2>
        <table className="mt-2 w-full text-sm">
          <thead>
            <tr className="border-b border-zinc-200 text-left text-xs font-medium uppercase text-muted-foreground dark:border-zinc-700">
              <th className="py-3 pr-6">{t("column.action")}</th>
              <th className="py-3 pr-6">{t("column.resource")}</th>
              <th className="py-3 pr-6">{t("column.status")}</th>
              <th className="py-3 pr-6">{t("column.date")}</th>
            </tr>
          </thead>
          <tbody>
            {resolved.map((a) => (
              <tr key={a.id} className="border-b border-zinc-100 dark:border-zinc-800">
                <td className="py-3 pr-6 text-zinc-900 dark:text-zinc-100">
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
            ))}
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
    </div>
  );
}
