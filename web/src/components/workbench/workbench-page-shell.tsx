"use client";

import type { ReactNode } from "react";
import { TabBar } from "@/components/ui/tab-bar";
import { FadeIn } from "@/components/motion/fade-in";
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
   * the body region scrolls as a single column (cardlist surfaces).
   */
  fillBleed?: boolean;
  /**
   * Optional secondary line beside the title. Use for terse context
   * ("Workspace canon vocabulary"). For an item count, use `count`.
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
  return (
    <div className="flex h-full flex-col">
      <header className="flex h-12 shrink-0 items-center justify-between gap-4 border-b border-divider px-4">
        <div className="flex min-w-0 items-baseline gap-3">
          <h1 className="shrink-0 text-lg font-semibold tracking-tight text-foreground-strong">
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
            <p className="truncate text-xs text-foreground-muted">{subtitle}</p>
          )}
        </div>
        {actions && (
          <div className="flex shrink-0 items-center gap-2">{actions}</div>
        )}
      </header>

      {filters && interactive && (
        <div className="flex min-h-10 shrink-0 items-center gap-2 border-b border-divider px-4 py-2">
          {filters}
        </div>
      )}

      {tabs && tabs.length > 0 && activeTab && onTabChange && (
        <div className="flex h-9 shrink-0 items-center border-b border-divider px-3">
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
        {children}
      </FadeIn>
    </div>
  );
}
