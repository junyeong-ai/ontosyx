"use client";

import { cn } from "@/lib/cn";
import type { NotificationTone } from "@/lib/store/types";

const TONE_PILL: Record<NotificationTone, string> = {
  neutral: "bg-surface-inset text-foreground-muted",
  info: "bg-info-surface text-info-foreground ring-1 ring-inset ring-info-border",
  warning:
    "bg-warning-surface text-warning-foreground ring-1 ring-inset ring-warning-border",
  danger:
    "bg-danger-surface text-danger-foreground ring-1 ring-inset ring-danger-border",
};

const TONE_DOT: Record<NotificationTone, string> = {
  neutral: "bg-foreground-muted",
  info: "bg-info-foreground",
  warning: "bg-warning-foreground",
  danger: "bg-danger-foreground",
};

interface NotificationBadgeProps {
  count: number;
  tone?: NotificationTone;
  /**
   * `pill` renders the count inside a tone-tinted chip with `99+`
   * rollup at 100. `dot` collapses to a 6px tone-tinted dot — used in
   * sidebar rail mode where the icon is the only visible target.
   */
  variant: "pill" | "dot";
  className?: string;
  ariaLabel?: string;
}

export function NotificationBadge({
  count,
  tone = "info",
  variant,
  className,
  ariaLabel,
}: NotificationBadgeProps) {
  if (count <= 0) return null;
  if (variant === "dot") {
    return (
      <span
        aria-label={ariaLabel}
        role={ariaLabel ? "status" : undefined}
        className={cn(
          "absolute end-1.5 top-1.5 h-2 w-2 rounded-full ring-2 ring-surface-raised",
          TONE_DOT[tone],
          className,
        )}
      />
    );
  }
  const display = count > 99 ? "99+" : String(count);
  return (
    <span
      aria-label={ariaLabel}
      className={cn(
        "inline-flex h-4 min-w-4 shrink-0 items-center justify-center rounded-full px-1 text-2xs font-semibold tabular-nums",
        TONE_PILL[tone],
        className,
      )}
    >
      {display}
    </span>
  );
}
