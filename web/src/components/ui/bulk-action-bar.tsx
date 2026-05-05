"use client";

// Sticky-bottom action bar for multi-select cohorts. Slides in
// when `count > 0`, slides out otherwise. Used by every list
// surface that pairs row checkboxes with a "Approve / Reject /
// Dismiss / …" workflow — knowledge base, stale-concept
// proposals, governance approvals so far.
//
// Why not `dialog` / `popover` semantics: the bar isn't a modal
// — the row content underneath stays interactive while the bar
// is up. `role="region"` + `aria-label` is the right surface for
// "important standing content" per ARIA APG.
//
// `pointer-events-none` while hidden so a mid-fade click can't
// fire an action. The buttons inside re-enable pointer events
// individually so they're hit-testable as soon as the slide-in
// completes.

import { Button, type ButtonVariant } from "@/components/ui/button";
import { cn } from "@/lib/cn";

/**
 * One bar action. The caller supplies a pre-translated `label`
 * — i18n is the call-site's concern, not this primitive's — and
 * a `variant` matching the underlying `<Button>` palette so a
 * destructive bulk action can render with the right tonal weight.
 */
export interface BulkAction {
  /** Stable React key + aria action target. */
  key: string;
  /** Pre-translated label (caller invokes `t()`). */
  label: string;
  /** Forwarded to `<Button variant>`. Defaults to `outline`. */
  variant?: ButtonVariant;
  /** Click handler. The bar disables every action while `pending`. */
  onClick: () => void;
}

export interface BulkActionBarProps {
  /** Number of selected items. The bar is hidden when `0`. */
  count: number;
  /**
   * Pre-translated count label, e.g. `"3 selected"` /
   * `"3건 선택됨"`. Caller is responsible for i18n + plural
   * formatting so the bar is locale-agnostic.
   */
  countLabel: string;
  /** Pre-translated label for the clear button. */
  clearLabel: string;
  /** Pre-translated `aria-label` for the bar's region. */
  ariaLabel: string;
  /**
   * Action buttons rendered to the right of the count, before
   * the clear button. Order is preserved.
   */
  actions: readonly BulkAction[];
  /** Clears the selection. Always rendered as the last button. */
  onClear: () => void;
  /**
   * Mutation in flight. Disables every button (including
   * `Clear`) so the user can't queue a second action against the
   * same cohort while the first is still resolving.
   */
  pending: boolean;
}

export function BulkActionBar({
  count,
  countLabel,
  clearLabel,
  ariaLabel,
  actions,
  onClear,
  pending,
}: BulkActionBarProps) {
  const visible = count > 0;
  return (
    <div
      className={cn(
        "pointer-events-none fixed inset-x-0 bottom-6 z-presence mx-auto flex max-w-2xl",
        "items-center justify-between gap-3 rounded-xl border border-divider",
        "bg-surface-overlay px-4 py-3 shadow-2",
        "transition-all duration-[var(--duration-base)] ease-[var(--ease-out)]",
        visible ? "translate-y-0 opacity-100" : "translate-y-4 opacity-0",
      )}
      role="region"
      aria-label={ariaLabel}
      aria-hidden={!visible}
    >
      <span className="text-sm font-medium text-foreground-strong">
        {countLabel}
      </span>
      <div className="flex items-center gap-2">
        <Button
          variant="outline"
          size="sm"
          onClick={onClear}
          disabled={pending}
          className="pointer-events-auto"
        >
          {clearLabel}
        </Button>
        {actions.map((action) => (
          <Button
            key={action.key}
            variant={action.variant ?? "outline"}
            size="sm"
            onClick={action.onClick}
            disabled={pending}
            className="pointer-events-auto"
          >
            {action.label}
          </Button>
        ))}
      </div>
    </div>
  );
}
