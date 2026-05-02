"use client";

// Comment thread on an approval. The reviewer's decision-time
// rationale lands here as the first entry; pre-/post-decision
// discussion follows in the same stream.

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
      <div className="text-2xs font-semibold uppercase tracking-wider text-muted-foreground">
        {t("heading")}
      </div>

      {query.isLoading ? null : comments.length === 0 ? (
        <div className="text-xs italic text-muted-foreground">{t("empty")}</div>
      ) : (
        <ul className="flex flex-col gap-2">
          {comments.map((c) => (
            <li
              key={c.id}
              className="rounded-md border border-divider bg-surface-base px-3 py-2 text-xs"
            >
              <div className="flex items-center justify-between text-2xs text-muted-foreground">
                <span className="font-medium">
                  {c.author_name ?? t("unknownAuthor")}
                </span>
                <span>{new Date(c.created_at).toLocaleString()}</span>
              </div>
              <div className="mt-1 whitespace-pre-wrap text-foreground-strong">
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
            className="rounded-md border border-divider bg-surface-base px-3 py-1.5 text-xs transition-colors focus:border-brand-foreground focus:outline-none focus:ring-1 focus:ring-brand-foreground/50 dark:focus:border-brand-border"
          />
          <div className="flex justify-end">
            <button
              type="button"
              onClick={handlePost}
              disabled={mutation.isPending || draft.trim().length === 0}
              className="rounded-md bg-surface-base px-3 py-1.5 text-xs font-medium text-white hover:bg-surface-base disabled:cursor-not-allowed disabled:opacity-50-strong dark:hover:bg-surface-base"
            >
              {mutation.isPending ? t("posting") : t("addButton")}
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
