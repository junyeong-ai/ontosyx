"use client";

import { useCallback, useMemo, useState } from "react";

// ---------------------------------------------------------------------------
// useTypeFilter — toggle visibility of graph elements grouped by type
// ---------------------------------------------------------------------------
//
// Widgets and the explore canvas both need "hide all nodes of type X"
// visibility toggles fed by a legend. The hook keeps a hidden-set (not a
// visible-set) so a caller that hasn't interacted yet sees every type —
// visibility is the default, hiding is the opt-in.
//
// The filter functions are generic so callers pass their own node / edge
// shapes; the hook doesn't own the graph data, only the toggle state.

export interface UseTypeFilterResult<NodeT, EdgeT> {
  /** Every type seen in the node list (deterministic order from `allTypes`). */
  types: string[];
  /** Types the user has currently hidden. */
  hiddenTypes: Set<string>;
  /** Whether the user has hidden anything — useful for "reset" affordances. */
  isAnyHidden: boolean;
  toggle: (type: string) => void;
  setHidden: (type: string, hidden: boolean) => void;
  clear: () => void;
  /** Filter a node list to just the currently visible types. */
  filterNodes: (nodes: NodeT[]) => NodeT[];
  /**
   * Filter edges, keeping only those whose endpoints are both in the
   * visible set. Callers pass the list of surviving node ids so the hook
   * doesn't need to know the node shape beyond what `getNodeId` reveals.
   */
  filterEdges: (edges: EdgeT[], visibleNodeIds: ReadonlySet<string>) => EdgeT[];
}

export interface UseTypeFilterOptions<NodeT, EdgeT> {
  /** All types present in the source graph, in display order. */
  allTypes: string[];
  getNodeType: (node: NodeT) => string;
  getEdgeSource: (edge: EdgeT) => string;
  getEdgeTarget: (edge: EdgeT) => string;
}

export function useTypeFilter<NodeT, EdgeT>(
  options: UseTypeFilterOptions<NodeT, EdgeT>,
): UseTypeFilterResult<NodeT, EdgeT> {
  const { allTypes, getNodeType, getEdgeSource, getEdgeTarget } = options;
  const [hiddenTypes, setHiddenTypes] = useState<Set<string>>(() => new Set());

  const toggle = useCallback((type: string) => {
    setHiddenTypes((prev) => {
      const next = new Set(prev);
      if (next.has(type)) {
        next.delete(type);
      } else {
        next.add(type);
      }
      return next;
    });
  }, []);

  const setHidden = useCallback((type: string, hidden: boolean) => {
    setHiddenTypes((prev) => {
      const has = prev.has(type);
      if (hidden === has) return prev;
      const next = new Set(prev);
      if (hidden) {
        next.add(type);
      } else {
        next.delete(type);
      }
      return next;
    });
  }, []);

  const clear = useCallback(() => setHiddenTypes(new Set()), []);

  const filterNodes = useCallback(
    (nodes: NodeT[]) => {
      if (hiddenTypes.size === 0) return nodes;
      return nodes.filter((n) => !hiddenTypes.has(getNodeType(n)));
    },
    [hiddenTypes, getNodeType],
  );

  const filterEdges = useCallback(
    (edges: EdgeT[], visibleNodeIds: ReadonlySet<string>) => {
      return edges.filter(
        (e) => visibleNodeIds.has(getEdgeSource(e)) && visibleNodeIds.has(getEdgeTarget(e)),
      );
    },
    [getEdgeSource, getEdgeTarget],
  );

  const isAnyHidden = hiddenTypes.size > 0;

  const types = useMemo(() => allTypes.slice(), [allTypes]);

  return {
    types,
    hiddenTypes,
    isAnyHidden,
    toggle,
    setHidden,
    clear,
    filterNodes,
    filterEdges,
  };
}
