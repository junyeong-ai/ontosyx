"use client";

import { useCallback, useMemo } from "react";
import { useAppStore } from "@/lib/store";
import type { UseTypeFilterOptions, UseTypeFilterResult } from "./use-type-filter";
import { useTypeFilter } from "./use-type-filter";

// ---------------------------------------------------------------------------
// useDashboardTypeFilter — dashboard-scoped cross-widget type visibility
// ---------------------------------------------------------------------------
//
// Extends `useTypeFilter` with an optional dashboard-scoped backing
// store: when `dashboardId` is supplied, the hidden-types set lives in
// the Zustand `DashboardSlice` keyed by `dashboardId`, so every widget
// that mounts this hook with the same id shares the same hidden set.
// Toggling a type in one widget re-renders every other widget bound to
// the same dashboard.
//
// When `dashboardId` is null/undefined (the widget isn't mounted inside
// a dashboard — e.g. query panel, chat execution detail), the hook
// falls back to `useTypeFilter`'s local state so single-widget usage
// stays isolated and identical to the pre-cross-filter behaviour.
//
// Rules-of-hooks: both the local-state hook and the store selectors
// are called unconditionally; the conditional is only over *which* set
// of return values we expose. Breaking that would re-order hook calls
// on dashboard/non-dashboard mounts.

export interface UseDashboardTypeFilterOptions<NodeT, EdgeT>
  extends UseTypeFilterOptions<NodeT, EdgeT> {
  /**
   * Dashboard identifier that scopes the shared hidden-types set.
   * When `null` / `undefined`, falls back to widget-local state.
   */
  dashboardId?: string | null;
}

export function useDashboardTypeFilter<NodeT, EdgeT>(
  options: UseDashboardTypeFilterOptions<NodeT, EdgeT>,
): UseTypeFilterResult<NodeT, EdgeT> {
  const { allTypes, getNodeType, getEdgeSource, getEdgeTarget, dashboardId } =
    options;

  // Local-state fallback — always called so hook order is stable.
  const local = useTypeFilter({
    allTypes,
    getNodeType,
    getEdgeSource,
    getEdgeTarget,
  });

  // Store hooks — always called. When no dashboardId is supplied we
  // simply ignore the returned actions.
  const stored = useAppStore((s) =>
    dashboardId ? (s.dashboardTypeFilters[dashboardId] ?? EMPTY_ARRAY) : EMPTY_ARRAY,
  );
  const toggleDashboardType = useAppStore((s) => s.toggleDashboardType);
  const setDashboardTypeHidden = useAppStore((s) => s.setDashboardTypeHidden);
  const clearDashboardTypes = useAppStore((s) => s.clearDashboardTypes);

  // Materialise the stored array into a `Set` once per change —
  // downstream callers want the `Set` semantics of `useTypeFilter`
  // (`hiddenTypes.has(…)`), and re-creating the set on every render
  // would defeat the `React.useMemo` boundaries inside the graph
  // renderer.
  const storedHiddenTypes = useMemo(() => new Set(stored), [stored]);

  const toggle = useCallback(
    (type: string) => {
      if (dashboardId) toggleDashboardType(dashboardId, type);
      else local.toggle(type);
    },
    [dashboardId, toggleDashboardType, local],
  );

  const setHidden = useCallback(
    (type: string, hidden: boolean) => {
      if (dashboardId) setDashboardTypeHidden(dashboardId, type, hidden);
      else local.setHidden(type, hidden);
    },
    [dashboardId, setDashboardTypeHidden, local],
  );

  const clear = useCallback(() => {
    if (dashboardId) clearDashboardTypes(dashboardId);
    else local.clear();
  }, [dashboardId, clearDashboardTypes, local]);

  const types = useMemo(() => allTypes.slice(), [allTypes]);

  // Pick the hidden set + filter closures from the appropriate source.
  // The closures are trivial over the hidden-types set, so rebuilding
  // them here (rather than reaching into `local`'s internals) keeps
  // the two code paths structurally symmetric.
  const hiddenTypes = dashboardId ? storedHiddenTypes : local.hiddenTypes;

  const filterNodes = useCallback(
    (nodes: NodeT[]) => {
      if (hiddenTypes.size === 0) return nodes;
      return nodes.filter((n) => !hiddenTypes.has(getNodeType(n)));
    },
    [hiddenTypes, getNodeType],
  );

  const filterEdges = useCallback(
    (edges: EdgeT[], visibleNodeIds: ReadonlySet<string>) =>
      edges.filter(
        (e) => visibleNodeIds.has(getEdgeSource(e)) && visibleNodeIds.has(getEdgeTarget(e)),
      ),
    [getEdgeSource, getEdgeTarget],
  );

  return {
    types,
    hiddenTypes,
    isAnyHidden: hiddenTypes.size > 0,
    toggle,
    setHidden,
    clear,
    filterNodes,
    filterEdges,
  };
}

// Stable empty reference so the selector returns the same object across
// renders when a dashboardId resolves to an absent entry — Zustand uses
// strict equality to skip re-renders, and a fresh `[]` each call would
// retrigger every subscribed component.
const EMPTY_ARRAY: string[] = Object.freeze([]) as unknown as string[];
