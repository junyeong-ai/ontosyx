"use client";

import {
  useQuery,
  type UseQueryOptions,
} from "@tanstack/react-query";
import { listSavedPatterns, type SavedPattern } from "@/lib/api/queries";
import type { CursorPage } from "@/types/api";

// ---------------------------------------------------------------------------
// Query keys
// ---------------------------------------------------------------------------

export const savedPatternsKeys = {
  all: ["saved-patterns"] as const,
  lists: () => [...savedPatternsKeys.all, "list"] as const,
  list: (ontologyId: string, params?: { limit?: number }) =>
    [...savedPatternsKeys.lists(), ontologyId, params ?? {}] as const,
};

// ---------------------------------------------------------------------------
// Queries
// ---------------------------------------------------------------------------

export function useSavedPatterns(
  ontologyId: string | null | undefined,
  params?: { limit?: number },
  options?: Omit<
    UseQueryOptions<CursorPage<SavedPattern>>,
    "queryKey" | "queryFn"
  >,
) {
  return useQuery({
    queryKey: savedPatternsKeys.list(ontologyId ?? "", params),
    queryFn: () => listSavedPatterns(ontologyId as string, params),
    // Caller controls `enabled` via `options`; default is "enabled when
    // the ontology id is present".
    enabled: !!ontologyId,
    ...options,
  });
}
