"use client";

import {
  useQuery,
  type UseQueryOptions,
} from "@tanstack/react-query";
import {
  listSavedPatterns,
  type SavedPatternPage,
} from "@/lib/api/queries";

// ---------------------------------------------------------------------------
// Query keys
// ---------------------------------------------------------------------------

export const savedPatternsKeys = {
  all: ["saved-patterns"] as const,
  lists: () => [...savedPatternsKeys.all, "list"] as const,
  list: (params?: { limit?: number }) =>
    [...savedPatternsKeys.lists(), "workspace", params ?? {}] as const,
};

// ---------------------------------------------------------------------------
// Queries
// ---------------------------------------------------------------------------

export function useSavedPatterns(
  _ontologyId?: string | null,
  params?: { limit?: number },
  options?: Omit<
    UseQueryOptions<SavedPatternPage>,
    "queryKey" | "queryFn"
  >,
) {
  return useQuery({
    queryKey: savedPatternsKeys.list(params),
    queryFn: () => listSavedPatterns(params),
    ...options,
  });
}
