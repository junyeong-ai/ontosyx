"use client";

// Comment thread on an approval. The reviewer's decision-time
// rationale lands here as the first entry; pre-/post-decision
// discussion follows in the same stream.

import { useEffect, useState } from "react";
import { useTranslations } from "next-intl";
import { toast } from "@/components/ui/toast";
import { FormTextarea } from "@/components/ui/form-input";

import {
  useApprovalComments,
  useCreateApprovalComment,
} from "@/hooks/api/use-approval-comments";
import { useFormatters } from "@/hooks/use-formatters";

interface CommentThreadProps {
  approvalId: string;
  /** Resolved rows render the thread read-only — composing a new
   *  comment on a closed approval is intentionally not allowed
   *  (the thread is point-in-time decision evidence, not a chat
   *  room). */
  readOnly?: boolean;
}

export function CommentThread({ approvalId, readOnly = false }: CommentThreadProps) {
  const t = useTranslations("settings.governance.approvals.thread");
  const fmt = useFormatters();
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
  }, [query.isError, t]);

  const comments = query.data ?? [];

  return (
    <div className="flex flex-col gap-2">
      <div className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
        {t("heading")}
      </div>

      {query.isLoading ? null : comments.length === 0 ? (
        <div className="text-xs italic text-foreground-muted">{t("empty")}</div>
      ) : (
        <ul className="flex flex-col gap-2">
          {comments.map((c) => (
            <li
              key={c.id}
              className="rounded-md border border-divider bg-surface-base px-3 py-2 text-xs"
            >
              <div className="flex items-center justify-between text-2xs text-foreground-muted">
                <span className="font-medium">
                  {c.author_name ?? t("unknownAuthor")}
                </span>
                <span>{fmt.date(c.created_at)}</span>
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
          <FormTextarea
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            placeholder={t("addPlaceholder")}
            rows={2}
            density="settings"
          />
          <div className="flex justify-end">
            <button
              type="button"
              onClick={handlePost}
              disabled={mutation.isPending || draft.trim().length === 0}
              className="rounded-md bg-surface-base px-3 py-1.5 text-xs font-medium text-foreground-onbrand hover:bg-surface-base disabled:cursor-not-allowed disabled:opacity-50-strong"
            >
              {mutation.isPending ? t("posting") : t("addButton")}
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
