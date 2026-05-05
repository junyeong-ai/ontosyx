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
  /**
   * Page label for the screen-reader-only outline. The visual title
   * row was removed because the active sidebar mode already names
   * the page on every viewport (Linear / Foundry / Figma / VSCode
   * pattern). Pass it anyway so assistive tech keeps the document
   * outline; visually it's `sr-only`.
   */
  title: string;
  /**
   * When true, the body region runs `overflow-hidden flex flex-col`
   * so the children can fill the viewport and own their internal
   * scroll (master-detail surfaces, canvas). When false (default),
   * the body region scrolls as a single column and the shell wraps
   * the children with the standard container padding + max-width.
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
   * Total item count rendered as a counter inside the chrome row.
   * Dimmed when `pageState` is `loading`/`error` (data isn't
   * authoritative yet). Pass `undefined` to omit.
   */
  count?: number;
  /**
   * Primary header actions (right side of the chrome row). Always
   * visible — these are the page-level CTAs (e.g. "New Draft") that
   * should remain reachable in every state.
   */
  actions?: ReactNode;
  /**
   * Filter / search row rendered as a separate strip when there
   * are tabs (so tabs and filters don't fight for the same line).
   * When there are no tabs, filters fold into the chrome row beside
   * the count for a tighter layout. Hidden in
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

// Standardized horizontal padding — same on every chrome strip and
// the default body container so the layout reads as a single column
// from sidebar edge to gutter.
const ROW_PADDING_X = "px-4 sm:px-6 lg:px-8";

/**
 * Public padding tokens for `fillBleed` surfaces (master-detail,
 * canvas, multi-pane). Those bypass the shell's body container, so
 * any gate view they render — empty state, error state, "no
 * canonical ontology" warning — has to apply the same gutter
 * itself or it ends up flush against the sidebar.
 *
 * Compose with `cn()` when adding extra classes, e.g.:
 *   <div className={cn(WORKBENCH_GUTTER, "flex flex-col gap-4")}>
 *
 * Don't redefine the values inline — keep the single source of truth
 * here so chrome row, body container, and gate views stay locked
 * across every viewport.
 */
export const WORKBENCH_GUTTER_X = ROW_PADDING_X;
export const WORKBENCH_GUTTER = `${ROW_PADDING_X} py-6`;

export function WorkbenchPageShell<TId extends string = string>({
  title,
  fillBleed = false,
  bodyPadding = "default",
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
  const hasTabs = !!(tabs && tabs.length > 0 && activeTab && onTabChange);
  const filtersVisible = !!filters && interactive;
  // When tabs exist filters get their own strip; when there are no
  // tabs filters share the chrome row with the count + actions for
  // density.
  const filtersInChrome = filtersVisible && !hasTabs;
  const filtersStandalone = filtersVisible && hasTabs;
  const showChromeRow =
    hasTabs ||
    typeof count === "number" ||
    !!actions ||
    filtersInChrome;

  const bodyClass = fillBleed
    ? null
    : bodyPadding === "narrow"
      ? `mx-auto w-full max-w-3xl ${ROW_PADDING_X} py-6`
      : bodyPadding === "default"
        ? `mx-auto w-full max-w-7xl ${ROW_PADDING_X} py-6`
        : null;

  return (
    <div className="flex h-full flex-col">
      {/* Document outline anchor — visual title is absent on purpose
          (sidebar already names the page; an inline H1 was visual
          noise + duplicate landmark). Screen readers still get a
          stable heading so the page outline reads cleanly. */}
      <h1 className="sr-only">{title}</h1>

      {showChromeRow && (
        <div
          className={cn(
            "flex h-11 shrink-0 items-center justify-between gap-3 border-b border-divider",
            ROW_PADDING_X,
          )}
        >
          <div className="flex min-w-0 items-center gap-3">
            {hasTabs && (
              <TabBar
                tabs={[...tabs!]}
                activeTab={activeTab!}
                onTabChange={(id) => onTabChange!(id as TId)}
              />
            )}
            {typeof count === "number" && (
              <span
                className={cn(
                  "shrink-0 rounded-full bg-surface-inset px-2 py-0.5 text-2xs font-medium tabular-nums",
                  interactive
                    ? "text-foreground-muted"
                    : "text-foreground-muted/50",
                )}
                aria-live="polite"
              >
                {count}
              </span>
            )}
            {filtersInChrome && (
              <div className="flex min-w-0 items-center gap-2">{filters}</div>
            )}
          </div>
          {actions && (
            <div className="flex shrink-0 items-center gap-2">{actions}</div>
          )}
        </div>
      )}

      {filtersStandalone && (
        <div
          className={cn(
            "flex min-h-10 shrink-0 items-center gap-2 border-b border-divider py-2",
            ROW_PADDING_X,
          )}
        >
          {filters}
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
