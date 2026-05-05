"use client";

import { useQuery } from "@tanstack/react-query";

import { listCanonicalVersions } from "@/lib/api/ontology-branches";
import type { OntologyVersionsResponse } from "@/types/ontology-branches";

export const ontologyBranchesKeys = {
  all: ["ontology-branches"] as const,
  versions: () => [...ontologyBranchesKeys.all, "versions"] as const,
};

/**
 * Fetch the workspace's canonical version history. Read-only —
 * the only mutation surface that touches versions is the draft
 * complete path, which already invalidates `workspaceOntologyKeys`
 * and the version list keys via the existing hooks.
 */
export function useCanonicalVersions() {
  return useQuery<OntologyVersionsResponse>({
    queryKey: ontologyBranchesKeys.versions(),
    queryFn: () => listCanonicalVersions(),
    staleTime: 30_000,
  });
}
