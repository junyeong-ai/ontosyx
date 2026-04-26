"use client";

// Φ6 #2 proper — comment thread on an approval.
//
// Shown on the expanded panel of pending rows (alongside the
// reviewer-note textarea) and under resolved rows so a reader can
// audit the rationale + any follow-up discussion. The legacy
// `review_notes` column on the parent row still records the
// decision-time rationale; the backend's `review_approval`
// transactionally mirrors that note into this thread so the two
// surfaces never disagree (no best-effort log-and-swallow path —
// either both writes land or both roll back).

import { useEffect, useState } from "react";
import { useTranslations } from "next-intl";
import { toast } from "sonner";

import {
  useApprovalComments,
  useCreateApprovalComment,
} from "@/hooks/api/use-approval-comments";

interface CommentThreadProps {
  approvalId: string;
  /** Resolved rows render the thread read-only — composing a new
   *  comment on a closed approval is intentionally not allowed
   *  (the thread is point-in-time decision evidence, not a chat
   *  room). */
  readOnly?: boolean;
}

export function CommentThread({ approvalId, readOnly = false }: CommentThreadProps) {
  const t = useTranslations("settings.approvals.thread");
  const query = useApprovalComments(approvalId);
  const mutation = useCreateApprovalComment(approvalId);
  const [draft, setDraft] = useState("");

  const handlePost = async () => {
    const body = draft.trim();
    if (!body) return;
    try {
      await mutation.mutateAsync(body);
      setDraft("");
      toast.success(t("posted"));
    } catch {
      toast.error(t("postFailed"));
    }
  };

  // Toast on each fresh load failure rather than every render — `isError`
  // sticks across renders, so a naked `if` would flood the surface.
  // The dep is `query.errorUpdatedAt` (changes only when a new error
  // event arrives), not `isError` (a sticky boolean).
  useEffect(() => {
    if (query.isError) {
      toast.error(t("loadFailed"));
    }
  }, [query.isError, query.errorUpdatedAt, t]);

  const comments = query.data ?? [];

  return (
    <div className="flex flex-col gap-2">
      <div className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
        {t("heading")}
      </div>

      {query.isLoading ? null : comments.length === 0 ? (
        <div className="text-xs italic text-muted-foreground">{t("empty")}</div>
      ) : (
        <ul className="flex flex-col gap-2">
          {comments.map((c) => (
            <li
              key={c.id}
              className="rounded-md border border-zinc-200 bg-white px-3 py-2 text-xs dark:border-zinc-700 dark:bg-zinc-950"
            >
              <div className="flex items-center justify-between text-[10px] text-muted-foreground">
                <span className="font-mono">{c.author_id.slice(0, 8)}</span>
                <span>{new Date(c.created_at).toLocaleString()}</span>
              </div>
              <div className="mt-1 whitespace-pre-wrap text-zinc-900 dark:text-zinc-100">
                {c.body}
              </div>
            </li>
          ))}
        </ul>
      )}

      {!readOnly && (
        <div className="flex flex-col gap-1.5">
          <textarea
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            placeholder={t("addPlaceholder")}
            rows={2}
            className="rounded-md border border-zinc-300 bg-white px-3 py-1.5 text-xs transition-colors focus:border-emerald-500 focus:outline-none focus:ring-1 focus:ring-emerald-500/50 dark:border-zinc-700 dark:bg-zinc-900 dark:focus:border-emerald-400"
          />
          <div className="flex justify-end">
            <button
              type="button"
              onClick={handlePost}
              disabled={mutation.isPending || draft.trim().length === 0}
              className="rounded-md bg-zinc-700 px-3 py-1.5 text-xs font-medium text-white hover:bg-zinc-800 disabled:cursor-not-allowed disabled:opacity-50 dark:bg-zinc-200 dark:text-zinc-900 dark:hover:bg-white"
            >
              {mutation.isPending ? t("posting") : t("addButton")}
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
