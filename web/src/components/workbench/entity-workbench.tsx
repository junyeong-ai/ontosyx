"use client";

// EntityWorkbench — generic master-detail shell for any
// CRUD-shaped entity surface (glossary terms, code systems, value
// sets, concept maps, notation patterns, rules, …).
//
// The layout is a 3-pane grid: a list pane on the left, a detail
// pane in the middle, and an optional auxiliary pane (usage map,
// activity, references) on the right. The auxiliary pane is
// collapsible so narrow viewports degrade gracefully into a 2-pane
// view, and a stacked vertical layout below the desktop breakpoint
// keeps the surface usable at smaller widths without forcing every
// caller to ship its own responsive logic.
//
// Industry patterns this consolidates:
//   * Linear settings — left list, right inline editor, no modals.
//   * Stripe Dashboard — entity tables with side-pane detail.
//   * Notion / Sanity Studio — schema-driven master-detail.
//   * Figma variables / styles — list + property pane.
//
// The component is intentionally presentational: it doesn't fetch
// data or wire mutations. The hosting component owns the items
// array, the selection state, and the create/update/delete
// handlers — same shape we use for `WorkbenchPageShell` in the
// rest of the app, so the lifecycle stays predictable.

import { type ReactNode, useState } from "react";
import { ArrowRight01Icon } from "@hugeicons/core-free-icons";

import { IconButton } from "@/components/ui/icon-button";
import { cn } from "@/lib/cn";

export interface EntityWorkbenchProps<T> {
  /** Renders the list pane content. The host owns search/filter/group. */
  listPane: ReactNode;
  /** Renders the detail pane (header + body + save bar). */
  detailPane: ReactNode;
  /** Optional auxiliary pane (usage / activity / refs). Collapsible. */
  auxPane?: ReactNode;
  /** Default open state for the aux pane. Defaults to `true` if pane present. */
  auxDefaultOpen?: boolean;
  /** ARIA label for the aux toggle button. */
  auxToggleLabel?: string;
  /**
   * Width tokens for each pane. Tailwind `grid-cols-[…]` literals.
   * Defaults to `280px_minmax(0,1fr)_340px`. Aux pane width is
   * collapsed to `0` automatically when toggled off.
   */
  listWidth?: string;
  auxWidth?: string;
  /** Optional banner above the workbench (e.g. ambiguity hint). */
  banner?: ReactNode;
  /**
   * Currently-selected item — passed through purely so the host
   * can key-prop the detail pane externally; not used internally.
   */
  selected?: T | null;
}

export function EntityWorkbench<T>({
  listPane,
  detailPane,
  auxPane,
  auxDefaultOpen = true,
  auxToggleLabel,
  listWidth = "280px",
  auxWidth = "340px",
  banner,
}: EntityWorkbenchProps<T>) {
  const [auxOpen, setAuxOpen] = useState(auxDefaultOpen && Boolean(auxPane));

  const cols = auxPane
    ? auxOpen
      ? `${listWidth} minmax(0,1fr) ${auxWidth}`
      : `${listWidth} minmax(0,1fr) 0`
    : `${listWidth} minmax(0,1fr)`;

  return (
    <div className="flex h-full flex-col overflow-hidden bg-surface-base">
      {banner}
      <div
        className={cn(
          "grid min-h-0 flex-1 divide-x divide-divider transition-[grid-template-columns] duration-200",
        )}
        style={{ gridTemplateColumns: cols }}
      >
        <div className="h-full min-w-0 overflow-hidden">{listPane}</div>
        <div className="relative flex h-full min-w-0 flex-col overflow-hidden">
          {auxPane && (
            <IconButton
              size="sm"
              label={auxToggleLabel ?? ""}
              onClick={() => setAuxOpen((v) => !v)}
              icon={ArrowRight01Icon}
              className={cn(
                "absolute right-2 top-2 z-10",
                auxOpen && "rotate-180",
              )}
            />
          )}
          {detailPane}
        </div>
        {auxPane && (
          <div
            aria-hidden={!auxOpen}
            className={cn(
              "h-full min-w-0 overflow-hidden",
              !auxOpen && "pointer-events-none invisible",
            )}
          >
            {auxPane}
          </div>
        )}
      </div>
    </div>
  );
}
