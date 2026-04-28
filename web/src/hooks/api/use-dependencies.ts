"use client";

import { useQuery } from "@tanstack/react-query";

import {
  getDependencyGraph,
  type SchemaDependencyGraph,
} from "@/lib/api/dependencies";

export const dependencyKeys = {
  all: ["dependencies"] as const,
  graph: (ontologyId: string) =>
    [...dependencyKeys.all, "graph", ontologyId] as const,
};

/**
 * Fetch and cache the schema-level dependency graph for an
 * ontology. Cached aggressively — the graph derives from the
 * committed IR snapshot, so it only changes on commit; mutators
 * invalidate `dependencyKeys.graph(id)` after a successful edit.
 */
export function useDependencyGraph(ontologyId: string | null | undefined) {
  return useQuery<SchemaDependencyGraph>({
    queryKey: dependencyKeys.graph(ontologyId ?? "__none__"),
    queryFn: () => getDependencyGraph(ontologyId!),
    enabled: !!ontologyId,
    staleTime: 5 * 60 * 1000,
  });
}
