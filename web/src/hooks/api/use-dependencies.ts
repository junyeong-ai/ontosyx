"use client";

import { useQuery } from "@tanstack/react-query";

import {
  getDependencyGraph,
  type SchemaDependencyGraph,
} from "@/lib/api/dependencies";

export const dependencyKeys = {
  all: ["dependencies"] as const,
  graph: () => [...dependencyKeys.all, "graph", "workspace"] as const,
};

/**
 * Fetch and cache the schema-level dependency graph for an
 * workspace ontology. Cached aggressively — the graph derives from the
 * committed IR snapshot, so it only changes on commit; mutators
 * invalidate `dependencyKeys.graph()` after a successful edit.
 */
export function useDependencyGraph(_ontologyId?: string | null) {
  return useQuery<SchemaDependencyGraph>({
    queryKey: dependencyKeys.graph(),
    queryFn: () => getDependencyGraph("workspace"),
    staleTime: 5 * 60 * 1000,
  });
}
