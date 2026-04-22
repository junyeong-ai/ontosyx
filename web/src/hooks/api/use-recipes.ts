"use client";

import {
  useMutation,
  useQuery,
  useQueryClient,
  type UseQueryOptions,
} from "@tanstack/react-query";
import { createRecipe, deleteRecipe, listRecipes } from "@/lib/api/admin";
import type { AnalysisRecipe, CursorPage } from "@/types/api";

// ---------------------------------------------------------------------------
// Query keys
// ---------------------------------------------------------------------------

export const recipesKeys = {
  all: ["recipes"] as const,
  lists: () => [...recipesKeys.all, "list"] as const,
  list: (params?: { limit?: number }) =>
    [...recipesKeys.lists(), params ?? {}] as const,
};

// ---------------------------------------------------------------------------
// Queries
// ---------------------------------------------------------------------------

export function useRecipes(
  params?: { limit?: number },
  options?: Omit<
    UseQueryOptions<CursorPage<AnalysisRecipe>>,
    "queryKey" | "queryFn"
  >,
) {
  return useQuery({
    queryKey: recipesKeys.list(params),
    queryFn: () => listRecipes(params),
    ...options,
  });
}

// ---------------------------------------------------------------------------
// Mutations
// ---------------------------------------------------------------------------

export function useCreateRecipe() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (req: Omit<AnalysisRecipe, "id" | "created_by" | "created_at">) =>
      createRecipe(req),
    onSuccess: () => qc.invalidateQueries({ queryKey: recipesKeys.lists() }),
  });
}

export function useDeleteRecipe() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => deleteRecipe(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: recipesKeys.lists() }),
  });
}
