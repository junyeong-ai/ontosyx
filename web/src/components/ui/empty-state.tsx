import {
  AlertCircleIcon,
  ChartBarLineIcon,
  Clock04Icon,
  Search01Icon,
  SecurityLockIcon,
  SparklesIcon,
} from "@hugeicons/core-free-icons";
import { HugeiconsIcon, type IconSvgElement } from "@hugeicons/react";
import { cn } from "@/lib/cn";
import { Button } from "./button";

type EmptyStateVariant = "hero" | "compact";

/**
 * Semantic categorisation of an empty surface, *orthogonal* to
 * `variant` (which is purely density / layout). The two together
 * determine icon, tone, and what action chrome reads naturally:
 *
 *   - `no-data` (default) — the list is genuinely empty, prompt
 *     creation. Brand-tone icon. Most common kind.
 *   - `no-results` — the list isn't empty, but the active filter
 *     hides everything. Action is "clear filter", not "create".
 *     Distinct register matters: a "Create your first project" CTA
 *     on a filtered project list is a UX bug.
 *   - `no-permission` — the surface exists, the user can see it,
 *     but lacks read access. Lock-tone icon, no creation CTA;
 *     the recovery is "ask an admin".
 *   - `first-run` — onboarding moment for a workspace that's never
 *     been touched. Sparkle-tone icon, primary CTA leads into the
 *     intended first action.
 *   - `pending` — the data is *eventually* coming (review queue
 *     drained, scheduled job not fired yet). Clock-tone icon, copy
 *     reassures rather than prompting. No primary CTA.
 *   - `error` — the fetch surfaced a recoverable failure. Use
 *     `<ErrorState>` instead for hard failures; `error` here is for
 *     soft cases like "search backend timed out, list is empty".
 */
export type EmptyStateKind =
  | "no-data"
  | "no-results"
  | "no-permission"
  | "first-run"
  | "pending"
  | "error";

interface EmptyStateProps {
  /**
   * Override the default icon for the kind. Most callers should let
   * the kind choose so the visual language stays consistent —
   * supply a custom icon only when the surface has a domain-strong
   * icon (e.g. graph-shaped no-data on the Lineage tab).
   */
  icon?: IconSvgElement;
  title: string;
  description?: string;
  hint?: string;
  action?: { label: string; onClick: () => void };
  secondaryAction?: { label: string; onClick: () => void };
  /**
   * `hero` (default) is page-level: stretches to fill its container,
   * generous padding, large icon. `compact` is inline-friendly: sizes
   * to content with tighter padding and a smaller icon. Pick
   * `compact` inside a sidebar, popover, or inspector panel where a
   * full-height card would dominate.
   */
  variant?: EmptyStateVariant;
  /** Semantic kind (default `no-data`). Picks the default icon + tone. */
  kind?: EmptyStateKind;
  className?: string;
}

const KIND_DEFAULT_ICON: Record<EmptyStateKind, IconSvgElement> = {
  "no-data": ChartBarLineIcon,
  "no-results": Search01Icon,
  "no-permission": SecurityLockIcon,
  "first-run": SparklesIcon,
  pending: Clock04Icon,
  error: AlertCircleIcon,
};

const KIND_TONE: Record<EmptyStateKind, { wrap: string; icon: string }> = {
  // Brand tone for the affirmative kinds (data and onboarding).
  "no-data": {
    wrap: "bg-brand-surface",
    icon: "text-brand-foreground",
  },
  "no-results": {
    wrap: "bg-brand-surface",
    icon: "text-brand-foreground",
  },
  "first-run": {
    wrap: "bg-brand-surface",
    icon: "text-brand-foreground",
  },
  // Muted tones for the descriptive kinds.
  "no-permission": {
    wrap: "bg-surface-inset",
    icon: "text-foreground-muted",
  },
  pending: {
    wrap: "bg-surface-inset",
    icon: "text-foreground-muted",
  },
  // Warning tone for soft errors.
  error: {
    wrap: "bg-warning-surface",
    icon: "text-warning-foreground",
  },
};

const containerClass: Record<EmptyStateVariant, string> = {
  hero:    "h-full gap-3 p-8",
  compact: "gap-2 p-4",
};

const iconWrapClass: Record<EmptyStateVariant, string> = {
  hero:    "h-12 w-12",
  compact: "h-9 w-9",
};

const iconClass: Record<EmptyStateVariant, string> = {
  hero:    "h-5 w-5",
  compact: "h-4 w-4",
};

const titleClass: Record<EmptyStateVariant, string> = {
  hero:    "text-sm",
  compact: "text-xs",
};

export function EmptyState({
  icon,
  title,
  description,
  hint,
  action,
  secondaryAction,
  variant = "hero",
  kind = "no-data",
  className,
}: EmptyStateProps) {
  const resolvedIcon = icon ?? KIND_DEFAULT_ICON[kind];
  const tone = KIND_TONE[kind];
  return (
    <div
      role="status"
      className={cn(
        "flex flex-col items-center justify-center text-center",
        containerClass[variant],
        className,
      )}
    >
      {resolvedIcon && (
        <div
          className={cn(
            "flex items-center justify-center rounded-full",
            tone.wrap,
            iconWrapClass[variant],
          )}
        >
          <HugeiconsIcon
            icon={resolvedIcon}
            className={cn(tone.icon, iconClass[variant])}
            size="100%"
          />
        </div>
      )}
      <div className="max-w-sm">
        <p className={cn("font-medium text-foreground", titleClass[variant])}>
          {title}
        </p>
        {description && (
          <p className="mt-1 text-xs text-foreground-muted">{description}</p>
        )}
      </div>
      {(action || secondaryAction) && (
        <div className="flex items-center gap-2">
          {action && (
            <Button variant="primary" size="sm" onClick={action.onClick}>
              {action.label}
            </Button>
          )}
          {secondaryAction && (
            <Button variant="ghost" size="sm" onClick={secondaryAction.onClick}>
              {secondaryAction.label}
            </Button>
          )}
        </div>
      )}
      {hint && <p className="text-2xs text-foreground-muted">{hint}</p>}
    </div>
  );
}
