"use client";

import {
  useMutation,
  useQuery,
  useQueryClient,
  type UseQueryOptions,
} from "@tanstack/react-query";
import {
  createRecipe,
  deleteRecipe,
  listRecipes,
  updateRecipeStatus,
  type CreateRecipeRequest,
} from "@/lib/api/admin";
import type { AnalysisRecipePage, RecipeStatus } from "@/types/api";
import { useOptimisticMutation } from "@/hooks/api/use-optimistic-mutation";

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
    UseQueryOptions<AnalysisRecipePage>,
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
    mutationFn: (req: CreateRecipeRequest) => createRecipe(req),
    onSuccess: () => qc.invalidateQueries({ queryKey: recipesKeys.lists() }),
  });
}

/**
 * Delete a recipe with optimistic update — drops the row from the
 * caller's list snapshot immediately, rolls back on server error.
 * `params` must match the `useRecipes(params)` call backing the
 * surface so the optimistic delta lands on the same cache key;
 * the post-settle `recipesKeys.lists()` invalidation still
 * refreshes any sibling list view (e.g. the insights panel's
 * `limit: 50` snapshot).
 */
export function useDeleteRecipe(params?: { limit?: number }) {
  return useOptimisticMutation<string, void>({
    mutationFn: (id) => deleteRecipe(id),
    queryKeys: [recipesKeys.list(params)],
    optimisticUpdate: (prev, id) => {
      if (!isRecipePage(prev)) return prev;
      return { ...prev, items: prev.items.filter((r) => r.id !== id) };
    },
  });
}

/**
 * Update a recipe's lifecycle status (`draft` / `approved` /
 * `deprecated`) with optimistic feedback — the status pill flips
 * immediately, rolls back on error.
 */
export function useUpdateRecipeStatus(params?: { limit?: number }) {
  type Vars = { id: string; status: RecipeStatus };
  return useOptimisticMutation<Vars, void>({
    mutationFn: ({ id, status }) => updateRecipeStatus(id, status),
    queryKeys: [recipesKeys.list(params)],
    optimisticUpdate: (prev, { id, status }) => {
      if (!isRecipePage(prev)) return prev;
      return {
        ...prev,
        items: prev.items.map((r) => (r.id === id ? { ...r, status } : r)),
      };
    },
  });
}

function isRecipePage(value: unknown): value is AnalysisRecipePage {
  return (
    typeof value === "object" &&
    value !== null &&
    "items" in value &&
    Array.isArray((value as { items: unknown }).items)
  );
}
