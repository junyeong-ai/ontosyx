"use client";

// `<MergeBanner>` — surfaces when the local edit stack lands on top
// of a remote update that arrived mid-session, and lets the user
// reconcile.
//
// Without an explicit reconciliation surface, two collaborators
// editing the same entity see one of three failure modes:
//   1. Last-writer-wins silently — the user who saved second
//      wins, the first user's changes vanish without a beat.
//   2. Optimistic-locking 409 stacktrace bubbles up as a generic
//      "save failed" toast — no recovery path.
//   3. Server-merged write produces a Frankenstein object — neither
//      user can tell what happened to their edits.
//
// `<MergeBanner>` makes the remote-update event first-class. The
// surface is sticky at the top of the editing pane (above the form
// chrome) and offers three explicit affordances:
//
//   * **Keep mine** — rebase the local edit stack atop the new
//     server state and re-submit. Most common path.
//   * **Take theirs** — drop the local edits, accept the server
//     version verbatim. Used when the remote change is the
//     authoritative one.
//   * **Compare** — open a side-by-side diff dialog. Used when
//     the user wants to manually merge field by field.
//
// The component owns *only* the visual surface and the three
// callbacks — it does not know what "edit" means. The host
// (inspector, glossary editor, rule editor) wires the callbacks
// to its own command-stack semantics.

import { HugeiconsIcon } from "@hugeicons/react";
import { GitBranchIcon } from "@hugeicons/core-free-icons";
import { useTranslations } from "next-intl";

import { Button } from "@/components/ui/button";
import { cn } from "@/lib/cn";

export interface MergeBannerProps {
  /**
   * Display name of the user who shipped the remote update. The
   * banner uses this to make the conflict concrete — "Hyejin updated
   * this entity 12 seconds ago" reads better than "remote change".
   */
  remoteAuthorName: string;
  /**
   * Optional — what changed in the remote update. Free-form copy;
   * the host can render a one-line diff summary ("renamed `email`
   * to `primary_email`"), or omit for a generic banner.
   */
  remoteChangeSummary?: string;
  /** "Keep mine" handler — rebase local stack and re-submit. */
  onKeepLocal: () => void;
  /** "Take theirs" handler — drop local stack, accept server state. */
  onAcceptRemote: () => void;
  /** Optional — opens a side-by-side compare dialog. */
  onCompare?: () => void;
  /** Tone control. `warning` (default) for unresolved conflicts;
   *  `info` while the user is reviewing without urgency. */
  tone?: "warning" | "info";
  /** Pass `true` while a `keep` / `accept` mutation is in flight to
   *  disable the action buttons and prevent double-submit. */
  busy?: boolean;
  className?: string;
}

const TONE_CLASS: Record<NonNullable<MergeBannerProps["tone"]>, string> = {
  warning: "border-warning-border bg-warning-surface",
  info: "border-info-border bg-info-surface",
};

const TONE_ICON_CLASS: Record<NonNullable<MergeBannerProps["tone"]>, string> = {
  warning: "text-warning-foreground",
  info: "text-info-foreground",
};

export function MergeBanner({
  remoteAuthorName,
  remoteChangeSummary,
  onKeepLocal,
  onAcceptRemote,
  onCompare,
  tone = "warning",
  busy = false,
  className,
}: MergeBannerProps) {
  const t = useTranslations("collab.mergeBanner");

  return (
    <div
      role="alert"
      aria-live="polite"
      className={cn(
        "flex flex-col gap-2 rounded-lg border px-3 py-2.5 shadow-1",
        TONE_CLASS[tone],
        className,
      )}
    >
      <div className="flex items-start gap-2">
        <HugeiconsIcon
          icon={GitBranchIcon}
          className={cn("mt-0.5 h-4 w-4 shrink-0", TONE_ICON_CLASS[tone])}
          size="100%"
          aria-hidden="true"
        />
        <div className="flex-1 text-xs">
          <p className="font-semibold text-foreground-strong">
            {t("title", { author: remoteAuthorName })}
          </p>
          {remoteChangeSummary && (
            <p className="mt-0.5 text-foreground-muted">
              {remoteChangeSummary}
            </p>
          )}
          <p className="mt-0.5 text-foreground-muted">{t("description")}</p>
        </div>
      </div>
      <div className="flex flex-wrap items-center justify-end gap-2">
        {onCompare && (
          <Button
            variant="ghost"
            size="sm"
            onClick={onCompare}
            disabled={busy}
          >
            {t("compare")}
          </Button>
        )}
        <Button
          variant="ghost"
          size="sm"
          onClick={onAcceptRemote}
          disabled={busy}
        >
          {t("acceptRemote")}
        </Button>
        <Button
          variant="primary"
          size="sm"
          onClick={onKeepLocal}
          disabled={busy}
        >
          {t("keepLocal")}
        </Button>
      </div>
    </div>
  );
}
