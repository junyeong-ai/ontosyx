"use client";

import {
  createContext,
  useContext,
  useState,
  type ReactNode,
} from "react";
import { createPortal } from "react-dom";
import { TabBar } from "@/components/ui/tab-bar";
import { Heading } from "@/components/ui/heading";
import { FadeIn } from "@/components/motion/fade-in";
import { cn } from "@/lib/cn";
import { isInteractive, type PageState } from "@/components/layout/page-state";
import type { LucideIcon } from "lucide-react";

// Chrome-slot portal — active facet contributes actions / filters
// directly into the page chrome row so tabbed surfaces read as a
// single integrated row. Outside a shell (tests, standalone usage)
// the hooks fall through to inline rendering.

interface ChromeSlots {
  actionsEl: HTMLElement | null;
  filtersEl: HTMLElement | null;
}

const ChromeSlotsContext = createContext<ChromeSlots | null>(null);

function useChromeSlot(
  selector: (slots: ChromeSlots) => HTMLElement | null,
  node: ReactNode,
) {
  const slots = useContext(ChromeSlotsContext);
  const target = slots ? selector(slots) : null;
  if (!node) return null;
  if (!target) return node;
  return createPortal(node, target);
}

export function useChromeActions(node: ReactNode) {
  return useChromeSlot((s) => s.actionsEl, node);
}

export function useChromeFilters(node: ReactNode) {
  return useChromeSlot((s) => s.filtersEl, node);
}
export interface WorkbenchTab<TId extends string = string> {
  id: TId;
  label: string;
  icon?: LucideIcon;
  badge?: number;
}

interface WorkbenchPageShellProps<TId extends string = string> {
  /**
   * Page label for the screen-reader-only outline. The sidebar
   * mode highlight names list pages on screen, so the H1 stays
   * `sr-only` for AT consumption. When `headline` is set (entity-
   * detail pages), the H1 is promoted to visible since the sidebar
   * cannot name a row-level entity.
   */
  title: string;
  /**
   * Small text strip above the chrome row — typically a breadcrumb
   * back-link for entity-detail pages (`← Evaluation`). Hidden on
   * list pages where the sidebar already provides orientation.
   */
  eyebrow?: ReactNode;
  /**
   * Visible entity title for detail pages (run name, dataset name).
   * When present, the H1 surfaces visually in the chrome row beside
   * any `status` prop, replacing the tabs/count/filters slot. The
   * sidebar cannot name row-level entities, so detail pages opt
   * in here to give the page a stable identity.
   */
  headline?: ReactNode;
  /**
   * Status badge / pill rendered after `headline` in the chrome row.
   * Use the shared `<StatusPill />` primitive so the tone tokens stay
   * locked across surfaces.
   */
  status?: ReactNode;
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
  eyebrow,
  headline,
  status,
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
  const hasHeadline = !!headline;
  // Slot refs for the chrome-actions / chrome-filters portal targets.
  // `useState` over `useRef` so the first paint after mount triggers a
  // re-render of the consumer hooks — by the time the active facet
  // calls `useChromeActions`, the target element is in the DOM.
  const [actionsEl, setActionsEl] = useState<HTMLElement | null>(null);
  const [filtersEl, setFiltersEl] = useState<HTMLElement | null>(null);
  // Detail-page chrome (headline/status) is mutually exclusive with
  // list-page chrome (tabs/count/filters) — a row-level entity page
  // doesn't host its own tab navigation. Suppress list-mode slots
  // when a headline is present so the chrome row stays readable.
  const hasTabs =
    !hasHeadline && !!(tabs && tabs.length > 0 && activeTab && onTabChange);
  const filtersVisible = !hasHeadline && !!filters && interactive;
  // When tabs exist filters get their own strip; when there are no
  // tabs filters share the chrome row with the count + actions for
  // density.
  const filtersInChrome = filtersVisible && !hasTabs;
  const filtersStandalone = filtersVisible && hasTabs;
  const showCount = !hasHeadline && typeof count === "number";
  const showChromeRow =
    hasHeadline ||
    hasTabs ||
    showCount ||
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
    <ChromeSlotsContext.Provider value={{ actionsEl, filtersEl }}>
    <div className="flex h-full flex-col">
      {/* Document outline anchor. Sidebar names list pages so the
          heading stays `sr-only`; entity-detail pages (`headline` set)
          promote the H1 to visible since the sidebar can't name a
          row-level entity. The Heading primitive owns size scaling
          via the design tokens — call sites never set sizes inline. */}
      {!hasHeadline && <h1 className="sr-only">{title}</h1>}

      {eyebrow && (
        <div
          className={cn(
            "flex shrink-0 items-center pt-3 pb-1 text-2xs font-medium text-foreground-muted",
            ROW_PADDING_X,
          )}
        >
          {eyebrow}
        </div>
      )}

      {showChromeRow && (
        <div
          className={cn(
            "flex shrink-0 items-center justify-between gap-3 border-b border-divider",
            hasHeadline ? "min-h-12 py-2" : "h-11",
            ROW_PADDING_X,
          )}
        >
          <div className="flex min-w-0 items-center gap-3">
            {hasHeadline ? (
              <>
                <Heading level={1} size={5} className="truncate">
                  {headline}
                </Heading>
                {status && <div className="shrink-0">{status}</div>}
              </>
            ) : (
              <>
                {hasTabs && (
                  <TabBar
                    tabs={[...tabs!]}
                    activeTab={activeTab!}
                    onTabChange={(id) => onTabChange!(id as TId)}
                  />
                )}
                {showCount && (
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
                  <div
                    ref={setFiltersEl}
                    className="flex min-w-0 items-center gap-2"
                  >
                    {filters}
                  </div>
                )}
              </>
            )}
          </div>
          {/* Right cluster — `actions` prop renders inline; the same
              container is also the portal target for facet-owned
              chrome (`useChromeActions(...)`). The flex container
              holds whichever combination is present. Always rendered
              when the chrome row exists so the portal slot is
              available for facets to mount into. */}
          <div
            ref={setActionsEl}
            className="flex shrink-0 items-center gap-2"
          >
            {actions}
          </div>
        </div>
      )}

      {filtersStandalone && (
        <div
          ref={setFiltersEl}
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
    </ChromeSlotsContext.Provider>
  );
}
