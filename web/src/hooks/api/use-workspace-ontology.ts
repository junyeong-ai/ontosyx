"use client";

import {
  useMutation,
  useQuery,
  useQueryClient,
  type UseQueryOptions,
} from "@tanstack/react-query";

import {
  adoptGraph,
  getWorkspaceOntology,
  reindexSchema,
} from "@/lib/api/ontology";
import type { OntologyDetail, OntologyIR } from "@/types/api";

// ---------------------------------------------------------------------------
// Query keys — workspace × ontology is 1:1 so the key is constant.
// ---------------------------------------------------------------------------

export const workspaceOntologyKeys = {
  all: ["workspace-ontology"] as const,
  detail: () => [...workspaceOntologyKeys.all, "detail"] as const,
};

// ---------------------------------------------------------------------------
// Queries
// ---------------------------------------------------------------------------

/**
 * Fetch the workspace's canonical ontology — identity row,
 * current-version summary, and fully hydrated `OntologyIR`.
 * Returns `null` when the workspace has no canonical yet
 * (greenfield state).
 */
export function useWorkspaceOntology(
  options?: Omit<
    UseQueryOptions<OntologyDetail | null>,
    "queryKey" | "queryFn"
  >,
) {
  return useQuery({
    queryKey: workspaceOntologyKeys.detail(),
    queryFn: () => getWorkspaceOntology(),
    ...options,
  });
}

// ---------------------------------------------------------------------------
// Mutations
// ---------------------------------------------------------------------------

/**
 * Adopt the live graph as the workspace's canonical ontology.
 * On success, invalidates the workspace ontology query so callers
 * re-fetch the new state.
 */
export function useAdoptGraph() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (req: { name?: string; save?: boolean }) =>
      adoptGraph(req.name, req.save) as Promise<OntologyIR>,
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: workspaceOntologyKeys.all });
    },
  });
}

/**
 * Reindex the workspace ontology's label RAG. Invalidates the
 * detail query to reflect any updated `nodes_indexed` / meta
 * counts.
 */
export function useReindexSchema() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => reindexSchema(),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: workspaceOntologyKeys.detail() });
    },
  });
}
