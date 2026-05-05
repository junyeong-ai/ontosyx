"use client";

import {
  useMutation,
  useQuery,
  useQueryClient,
  type UseMutationOptions,
} from "@tanstack/react-query";

import {
  bulkRevokeAmbiguities,
  getAmbiguity,
  listAmbiguities,
  resolveAmbiguity,
  revokeAmbiguity,
  type AmbiguityMapping,
  type AmbiguityResolution,
  type AmbiguitySummary,
} from "@/lib/api/ambiguity";
import { useOptimisticMutation } from "./use-optimistic-mutation";

// ---------------------------------------------------------------------------
// Keys
// ---------------------------------------------------------------------------

export const ambiguitiesKeys = {
  all: ["ambiguities"] as const,
  list: () => [...ambiguitiesKeys.all, "list"] as const,
  detail: (id: string) => [...ambiguitiesKeys.all, "detail", id] as const,
};

// ---------------------------------------------------------------------------
// Queries
// ---------------------------------------------------------------------------

export function useAmbiguities() {
  return useQuery<{ items: AmbiguitySummary[] }>({
    queryKey: ambiguitiesKeys.list(),
    queryFn: () => listAmbiguities(),
  });
}

export function useAmbiguity(id: string | null) {
  return useQuery({
    queryKey: id ? ambiguitiesKeys.detail(id) : ambiguitiesKeys.detail("__none__"),
    queryFn: () =>
      id ? getAmbiguity(id) : Promise.reject(new Error("id required")),
    enabled: !!id,
  });
}

// ---------------------------------------------------------------------------
// Mutations
// ---------------------------------------------------------------------------

export interface ResolveVariables {
  id: string;
  mapping: AmbiguityMapping;
}

export function useResolveAmbiguity(
  options?: UseMutationOptions<AmbiguityResolution, Error, ResolveVariables>,
) {
  const queryClient = useQueryClient();
  const { onSuccess, ...rest } = options ?? {};
  return useMutation<AmbiguityResolution, Error, ResolveVariables>({
    ...rest,
    mutationFn: ({ id, mapping }) => resolveAmbiguity(id, mapping),
    onSuccess: (...args) => {
      queryClient.invalidateQueries({ queryKey: ambiguitiesKeys.all });
      onSuccess?.(...args);
    },
  });
}

export function useRevokeAmbiguity(
  options?: UseMutationOptions<{ revoked: boolean }, Error, string>,
) {
  const queryClient = useQueryClient();
  const { onSuccess, ...rest } = options ?? {};
  return useMutation<{ revoked: boolean }, Error, string>({
    ...rest,
    mutationFn: (id) => revokeAmbiguity(id),
    onSuccess: (...args) => {
      queryClient.invalidateQueries({ queryKey: ambiguitiesKeys.all });
      onSuccess?.(...args);
    },
  });
}

/**
 * Bulk-revoke every selected resolution in one round-trip. Optimism
 * mirrors the single-id path: clear `active_resolution` on each
 * matching summary so the row drops out of the "resolved" tab into
 * "pending" instantly. The BE caps `ids.len()` at 100 (the typed
 * `bulk_limit_exceeded` gate); callers split larger cohorts.
 */
export function useBulkRevokeAmbiguities() {
  type Vars = { ids: string[] };
  return useOptimisticMutation<Vars, { revoked: number }>({
    mutationFn: ({ ids }) => bulkRevokeAmbiguities(ids),
    queryKeys: [ambiguitiesKeys.list()],
    optimisticUpdate: (prev, { ids }) => {
      if (!isAmbiguityList(prev)) return prev;
      const idSet = new Set(ids);
      return {
        ...prev,
        items: prev.items.map((s) =>
          idSet.has(s.context.id) && s.active_resolution
            ? { ...s, active_resolution: null }
            : s,
        ),
      };
    },
  });
}

function isAmbiguityList(value: unknown): value is { items: AmbiguitySummary[] } {
  return (
    typeof value === "object" &&
    value !== null &&
    "items" in value &&
    Array.isArray((value as { items: unknown }).items)
  );
}
