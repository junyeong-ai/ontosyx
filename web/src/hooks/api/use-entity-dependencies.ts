"use client";

import { useMemo } from "react";

import { useDependencyGraph } from "./use-dependencies";
import {
  dependentsOf,
  referencesOf,
  type DependencyEdge,
  type SchemaEntityRef,
} from "@/lib/api/dependencies";
import { entityRefKey } from "@/lib/api/dependencies";

export interface EntityDependencies {
  /** Entities that reference `ref`. Empty when nothing depends on it. */
  inbound: readonly DependencyEdge[];
  /** Entities that `ref` references. Empty when it holds no outbound refs. */
  outbound: readonly DependencyEdge[];
  isLoading: boolean;
  error: Error | null;
}

/**
 * Per-entity slice of the workspace's
 * [`SchemaDependencyGraph`](../../lib/api/dependencies.ts).
 *
 * Both directions derive from a single cached query
 * ([`useDependencyGraph`]) so all consumers share the same network
 * round-trip; switching the highlighted entity is a synchronous
 * recompute, not a refetch.
 *
 * Generic on entity kind — pass any [`SchemaEntityRef`] (NodeType,
 * EdgeType, Property, Rule, Mapping, …). Domain-specific surfaces
 * — the Inspector's Lineage / Dependents tabs, the Domain Context
 * page's Lineage section — wrap this with the appropriate ref
 * shape at the call site.
 */
export function useEntityDependencies(
  ontologyId: string | null | undefined,
  ref: SchemaEntityRef | null,
): EntityDependencies {
  const { data: graph, isLoading, error } = useDependencyGraph(ontologyId);
  const refKey = ref ? entityRefKey(ref) : null;
  return useMemo(() => {
    if (!graph || !ref) {
      return {
        inbound: [] as readonly DependencyEdge[],
        outbound: [] as readonly DependencyEdge[],
        isLoading,
        error: error as Error | null,
      };
    }
    return {
      inbound: dependentsOf(graph, ref),
      outbound: referencesOf(graph, ref),
      isLoading,
      error: error as Error | null,
    };
    // refKey collapses the structural ref into a stable string so
    // the memo doesn't churn on identity-only changes.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [graph, refKey, isLoading, error]);
}
