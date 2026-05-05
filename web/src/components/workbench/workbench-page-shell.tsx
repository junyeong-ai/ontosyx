"use client";

import type { ReactNode } from "react";
import { HugeiconsIcon } from "@hugeicons/react";
import { InformationCircleIcon } from "@hugeicons/core-free-icons";
import { TabBar } from "@/components/ui/tab-bar";
import { FadeIn } from "@/components/motion/fade-in";
import { Tooltip } from "@/components/ui/tooltip";
import { cn } from "@/lib/cn";
import { isInteractive, type PageState } from "@/components/layout/page-state";
import type { IconSvgElement } from "@hugeicons/react";

export interface WorkbenchTab<TId extends string = string> {
  id: TId;
  label: string;
  icon?: IconSvgElement;
  badge?: number;
}

interface WorkbenchPageShellProps<TId extends string = string> {
  title: string;
  /**
   * When true, the body region runs `overflow-hidden flex flex-col`
   * so the children can fill the viewport and own their internal
   * scroll (master-detail surfaces, canvas). When false (default),
   * the body region scrolls as a single column with the standard
   * container padding + max-width applied (cardlist surfaces).
   */
  fillBleed?: boolean;
  /**
   * Body container variant for `fillBleed === false` surfaces.
   *
   * - `"default"` (default) — `mx-auto max-w-7xl px-4 sm:px-6 lg:px-8
   *   py-6` so dashboards / cardlists / data tables get a comfortable
   *   reading column with breathing room from the chrome.
   * - `"narrow"` — same horizontal padding but `max-w-3xl`. For
   *   form-heavy or read-heavy pages where wide measure hurts.
   * - `"flush"` — no wrapper. Page owns its own padding (rare;
   *   prefer `fillBleed` for full-bleed master-detail surfaces).
   *
   * Ignored when `fillBleed === true`.
   */
  bodyPadding?: "default" | "narrow" | "flush";
  /**
   * Optional descriptive context. Surfaces as a hover/focus tooltip
   * on a small info icon next to the title rather than inline copy
   * — matches the Linear / Stripe / Foundry pattern where the page
   * title earns its own line and the description is on-demand. The
   * sidebar already names the page and a verbose inline subtitle
   * pushed the count + actions row off-balance on every reload.
   * For an item count, use `count`.
   */
  subtitle?: string;
  /**
   * Total item count rendered as a counter beside the title. Dimmed
   * when `pageState` is `loading`/`error` (data isn't authoritative
   * yet). Pass `undefined` to omit.
   */
  count?: number;
  /**
   * Primary header actions (right side). Always visible — these are
   * the page-level CTAs (e.g. "New Project") that should remain
   * reachable in every state.
   */
  actions?: ReactNode;
  /**
   * Filter / search row rendered below the title. Hidden in
   * `loading` / `error` / `empty` states because there is nothing
   * to filter. Visible in `data` and `filtered-empty` so the user
   * can adjust or clear the active filter set.
   */
  filters?: ReactNode;
  tabs?: ReadonlyArray<WorkbenchTab<TId>>;
  activeTab?: TId;
  onTabChange?: (id: TId) => void;
  /**
   * Drives chrome visibility. Default is `data` (full chrome shown)
   * for pages whose state is intrinsically static.
   */
  pageState?: PageState;
  children: ReactNode;
}

const DEFAULT_STATE: PageState = { kind: "data" };

export function WorkbenchPageShell<TId extends string = string>({
  title,
  fillBleed = false,
  bodyPadding = "default",
  subtitle,
  count,
  actions,
  filters,
  tabs,
  activeTab,
  onTabChange,
  pageState = DEFAULT_STATE,
  children,
}: WorkbenchPageShellProps<TId>) {
  const interactive = isInteractive(pageState);
  const bodyClass = fillBleed
    ? null
    : bodyPadding === "narrow"
      ? "mx-auto w-full max-w-3xl px-4 sm:px-6 lg:px-8 py-6"
      : bodyPadding === "default"
        ? "mx-auto w-full max-w-7xl px-4 sm:px-6 lg:px-8 py-6"
        : null;
  return (
    <div className="flex h-full flex-col">
      <header className="flex h-11 shrink-0 items-center justify-between gap-4 border-b border-divider px-4 sm:px-6 lg:px-8">
        <div className="flex min-w-0 items-center gap-2">
          <h1 className="shrink-0 text-base font-semibold tracking-tight text-foreground-strong">
            {title}
          </h1>
          {typeof count === "number" && (
            <span
              className={cn(
                "shrink-0 rounded-full bg-surface-inset px-2 py-0.5 text-2xs font-medium tabular-nums",
                interactive ? "text-foreground-muted" : "text-foreground-muted/50",
              )}
              aria-live="polite"
            >
              {count}
            </span>
          )}
          {subtitle && (
            <Tooltip content={subtitle} side="bottom">
              <button
                type="button"
                aria-label={subtitle}
                className="shrink-0 rounded p-0.5 text-foreground-muted/70 hover:text-foreground-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand-foreground/40"
              >
                <HugeiconsIcon
                  icon={InformationCircleIcon}
                  className="h-3.5 w-3.5"
                  size="100%"
                />
              </button>
            </Tooltip>
          )}
        </div>
        {actions && (
          <div className="flex shrink-0 items-center gap-2">{actions}</div>
        )}
      </header>

      {filters && interactive && (
        <div className="flex min-h-10 shrink-0 items-center gap-2 border-b border-divider px-4 py-2 sm:px-6 lg:px-8">
          {filters}
        </div>
      )}

      {tabs && tabs.length > 0 && activeTab && onTabChange && (
        <div className="flex h-9 shrink-0 items-center border-b border-divider px-3 sm:px-5 lg:px-7">
          <TabBar
            tabs={[...tabs]}
            activeTab={activeTab}
            onTabChange={(id) => onTabChange(id as TId)}
          />
        </div>
      )}

      <FadeIn
        className={cn(
          "flex-1",
          fillBleed
            ? "flex min-h-0 flex-col overflow-hidden"
            : "overflow-auto",
        )}
      >
        {bodyClass ? <div className={bodyClass}>{children}</div> : children}
      </FadeIn>
    </div>
  );
}
