"use client";

// SaveBar — sticky footer for entity editors. Surfaces the dirty
// state, last-saved timestamp, and the save/discard actions.
// Industry pattern (Linear, Sanity, Notion property editors,
// GitHub settings): editing happens inline with no submit button
// per field; the save bar slides in from the bottom only when
// there are unsaved changes.
//
// Render this at the bottom of any persistent detail pane. The
// component does NOT manage form state — it's a presentational
// shell. Caller passes `dirty`, `pending`, `lastSavedAt`, and the
// two action callbacks.

import { useTranslations } from "next-intl";

import { Button } from "@/components/ui/button";
import { cn } from "@/lib/cn";

interface SaveBarProps {
  dirty: boolean;
  pending: boolean;
  /** ISO timestamp of the last successful save, if any. */
  lastSavedAt?: string | null;
  onSave: () => void;
  onDiscard: () => void;
  /** Optional badge slot — render an inline error / warning here. */
  notice?: React.ReactNode;
}

export function SaveBar({
  dirty,
  pending,
  lastSavedAt,
  onSave,
  onDiscard,
  notice,
}: SaveBarProps) {
  const t = useTranslations("forms.saveBar");
  const visible = dirty || pending;
  return (
    <div
      className={cn(
        "border-t border-divider bg-surface-base px-4 py-2 transition-all duration-[var(--duration-quick)]",
        visible ? "translate-y-0 opacity-100" : "pointer-events-none translate-y-2 opacity-0",
      )}
      aria-hidden={!visible}
    >
      <div className="flex items-center gap-3">
        <div className="min-w-0 flex-1 text-2xs text-foreground-muted">
          {pending ? (
            <span className="font-medium text-foreground-strong">
              {t("saving")}
            </span>
          ) : dirty ? (
            <span className="font-medium text-warning-foreground">
              {t("unsavedChanges")}
            </span>
          ) : lastSavedAt ? (
            <RelativeTimestamp iso={lastSavedAt} />
          ) : null}
          {notice && <span className="ms-2">{notice}</span>}
        </div>
        <Button
          variant="ghost"
          size="sm"
          onClick={onDiscard}
          disabled={pending || !dirty}
        >
          {t("discard")}
        </Button>
        <Button
          variant="primary"
          size="sm"
          onClick={onSave}
          disabled={pending || !dirty}
          loading={pending}
        >
          {t("save")}
        </Button>
      </div>
    </div>
  );
}

// Self-contained relative timestamp — re-uses Intl.RelativeTimeFormat
// in the active locale rather than depending on a date-fns import.
// "방금 저장됨" / "Saved 5 min ago" depending on locale chain.
function RelativeTimestamp({ iso }: { iso: string }) {
  const t = useTranslations("forms.saveBar");
  const ts = new Date(iso).getTime();
  const seconds = Math.max(0, (Date.now() - ts) / 1000);
  if (seconds < 60) {
    return <span>{t("savedJustNow")}</span>;
  }
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) {
    return <span>{t("savedMinutesAgo", { count: minutes })}</span>;
  }
  const hours = Math.floor(minutes / 60);
  if (hours < 24) {
    return <span>{t("savedHoursAgo", { count: hours })}</span>;
  }
  const days = Math.floor(hours / 24);
  return <span>{t("savedDaysAgo", { count: days })}</span>;
}
