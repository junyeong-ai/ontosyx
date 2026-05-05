"use client";

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import {
  diffDraftAgainstCanonical,
  listCanonicalVersions,
  rebaseDraftAgainstCanonical,
} from "@/lib/api/ontology-branches";
import { ontologyDraftsKeys } from "@/hooks/api/use-ontology-drafts";
import type {
  OntologyDiffSummary,
  OntologyVersionsResponse,
  RebaseDraftResponse,
} from "@/types/ontology-branches";

export const ontologyBranchesKeys = {
  all: ["ontology-branches"] as const,
  versions: () => [...ontologyBranchesKeys.all, "versions"] as const,
  diffCanonical: (draftId: string) =>
    [...ontologyBranchesKeys.all, "diff-canonical", draftId] as const,
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

/**
 * Diff a draft against the workspace canonical head. Lazily
 * fetched (`enabled`) so the caller can mount the hook before
 * the draft id is known.
 */
export function useDraftDiffAgainstCanonical(
  draftId: string | null | undefined,
) {
  return useQuery<OntologyDiffSummary>({
    queryKey: ontologyBranchesKeys.diffCanonical(draftId ?? ""),
    queryFn: () => {
      if (!draftId) {
        throw new Error("draft id is required");
      }
      return diffDraftAgainstCanonical(draftId);
    },
    enabled: !!draftId,
  });
}

export function useRebaseDraft() {
  const qc = useQueryClient();
  return useMutation<RebaseDraftResponse, Error, string>({
    mutationFn: (draftId) => rebaseDraftAgainstCanonical(draftId),
    onSuccess: (_data, draftId) => {
      qc.invalidateQueries({ queryKey: ontologyDraftsKeys.all });
      qc.invalidateQueries({
        queryKey: ontologyBranchesKeys.diffCanonical(draftId),
      });
    },
  });
}
