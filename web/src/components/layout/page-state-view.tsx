"use client";

import type { ReactNode } from "react";
import type { LucideIcon as IconSvgElement } from "lucide-react";
import { EmptyState } from "@/components/ui/empty-state";
import { ErrorState } from "@/components/ui/error-state";
import type { PageState } from "./page-state";

interface EmptyContent {
  icon?: IconSvgElement;
  title: string;
  description?: string;
  hint?: string;
  action?: { label: string; onClick: () => void };
  secondaryAction?: { label: string; onClick: () => void };
}

interface FilteredEmptyContent {
  icon?: IconSvgElement;
  title: string;
  description?: string;
  /** Reset-filters CTA label. The shell wires `onClick` from the page state. */
  clearLabel: string;
}

interface ErrorContent {
  title: string;
  description?: string;
  retryLabel: string;
}

interface PageStateViewProps {
  state: PageState;
  /** Body to render when `state.kind === "data"`. */
  children: ReactNode;
  /**
   * Loading placeholder. Pages provide a layout-faithful skeleton —
   * the same shape as the eventual `data` body so there is no
   * cumulative layout shift when content arrives.
   */
  skeleton: ReactNode;
  /**
   * Slots for non-`data` states. Each is optional at the type level
   * because not every page reaches every state (e.g. a page with no
   * filters never produces `filtered-empty`). Reaching a state without
   * the matching slot is a contract violation — the component throws
   * in development and renders an empty fragment in production so the
   * fallback is loud enough to notice but doesn't crash a live build.
   *
   * Contract: provide every slot the caller's `pageState` factory can
   * actually produce.
   */
  error?: ErrorContent;
  empty?: EmptyContent;
  filteredEmpty?: FilteredEmptyContent;
}

/**
 * Map a `PageState` to the matching primitive (`SkeletonX`, `EmptyState`,
 * `ErrorState`, or the live body). Pages compose their `pageState` from
 * query state + filter state and hand the same value to this component;
 * the discriminated union keeps `onRetry` / `onClearFilters` typed at
 * each branch so handlers can never go missing.
 */
export function PageStateView({
  state,
  children,
  skeleton,
  empty,
  filteredEmpty,
  error,
}: PageStateViewProps) {
  switch (state.kind) {
    case "loading":
      return <>{skeleton}</>;
    case "error":
      if (!error) {
        throwMissingSlot(state.kind, "error");
        return null;
      }
      return (
        <ErrorState
          title={error.title}
          description={error.description}
          onRetry={state.onRetry}
          retryLabel={error.retryLabel}
        />
      );
    case "empty":
      if (!empty) {
        throwMissingSlot(state.kind, "empty");
        return null;
      }
      return (
        <EmptyState
          icon={empty.icon}
          title={empty.title}
          description={empty.description}
          hint={empty.hint}
          action={empty.action}
          secondaryAction={empty.secondaryAction}
        />
      );
    case "filtered-empty":
      if (!filteredEmpty) {
        throwMissingSlot(state.kind, "filteredEmpty");
        return null;
      }
      return (
        <EmptyState
          icon={filteredEmpty.icon}
          title={filteredEmpty.title}
          description={filteredEmpty.description}
          action={{
            label: filteredEmpty.clearLabel,
            onClick: state.onClearFilters,
          }}
        />
      );
    case "data":
      return <>{children}</>;
    default: {
      const _exhaustive: never = state;
      return _exhaustive;
    }
  }
}

function throwMissingSlot(kind: PageState["kind"], slot: string): void {
  const message = `<PageStateView> reached state.kind="${kind}" but the "${slot}" slot is missing. The page that produced this state must pass the matching content prop.`;
  if (process.env.NODE_ENV !== "production") {
    throw new Error(message);
  }
  console.error(message);
}
