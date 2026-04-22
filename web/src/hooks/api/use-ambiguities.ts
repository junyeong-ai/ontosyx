"use client";

import {
  useMutation,
  useQuery,
  useQueryClient,
  type UseMutationOptions,
} from "@tanstack/react-query";

import {
  getAmbiguity,
  listAmbiguities,
  resolveAmbiguity,
  revokeAmbiguity,
  type AmbiguityMapping,
  type AmbiguityResolution,
  type AmbiguitySummary,
} from "@/lib/api/ambiguity";

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
