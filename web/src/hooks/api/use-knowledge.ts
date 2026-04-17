"use client";

import {
  useInfiniteQuery,
  useMutation,
  useQuery,
  useQueryClient,
  type UseQueryOptions,
} from "@tanstack/react-query";
import {
  createKnowledge,
  deleteKnowledge,
  knowledgeStats,
  listKnowledge,
  updateKnowledgeStatus,
} from "@/lib/api/knowledge";
import type {
  CursorPage,
  KnowledgeCreateRequest,
  KnowledgeEntry,
  KnowledgeStats,
  KnowledgeStatus,
} from "@/types/api";

// ---------------------------------------------------------------------------
// Query keys
// ---------------------------------------------------------------------------

export interface KnowledgeListFilters {
  ontology_name?: string;
  kind?: string;
  status?: string;
  limit?: number;
}

export const knowledgeKeys = {
  all: ["knowledge"] as const,
  lists: () => [...knowledgeKeys.all, "list"] as const,
  list: (filters?: KnowledgeListFilters) =>
    [...knowledgeKeys.lists(), filters ?? {}] as const,
  infinite: (filters?: KnowledgeListFilters) =>
    [...knowledgeKeys.lists(), "infinite", filters ?? {}] as const,
  details: () => [...knowledgeKeys.all, "detail"] as const,
  detail: (id: string) => [...knowledgeKeys.details(), id] as const,
  stats: () => [...knowledgeKeys.all, "stats"] as const,
};

// ---------------------------------------------------------------------------
// Queries
// ---------------------------------------------------------------------------

export function useKnowledge(
  filters?: KnowledgeListFilters,
  options?: Omit<
    UseQueryOptions<CursorPage<KnowledgeEntry>>,
    "queryKey" | "queryFn"
  >,
) {
  return useQuery({
    queryKey: knowledgeKeys.list(filters),
    queryFn: () => listKnowledge(filters),
    ...options,
  });
}

/**
 * Infinite-scroll knowledge list.
 *
 * Why `useInfiniteQuery` here (and not for dashboards/projects): the
 * knowledge settings page explicitly exposes a "Load more" button and
 * accumulates entries across pages.
 */
export function useKnowledgeInfinite(filters?: KnowledgeListFilters) {
  return useInfiniteQuery({
    queryKey: knowledgeKeys.infinite(filters),
    queryFn: ({ pageParam }) =>
      listKnowledge({ ...filters, cursor: pageParam as string | undefined }),
    initialPageParam: undefined as string | undefined,
    getNextPageParam: (last) => last.next_cursor ?? undefined,
  });
}

export function useKnowledgeStats(
  options?: Omit<UseQueryOptions<KnowledgeStats>, "queryKey" | "queryFn">,
) {
  return useQuery({
    queryKey: knowledgeKeys.stats(),
    queryFn: () => knowledgeStats(),
    ...options,
  });
}

// ---------------------------------------------------------------------------
// Mutations — with optimistic updates
// ---------------------------------------------------------------------------

/**
 * Create a knowledge entry. Optimistic pattern not applied — the backend
 * assigns the id and we want the real record before showing it.
 */
export function useCreateKnowledge() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (req: KnowledgeCreateRequest) => createKnowledge(req),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: knowledgeKeys.lists() });
      qc.invalidateQueries({ queryKey: knowledgeKeys.stats() });
    },
  });
}

/**
 * Delete a knowledge entry with optimistic update.
 *
 * Why optimistic: deletes are common in the review workflow; waiting for
 * the round-trip before removing the row feels laggy. On failure we roll
 * back the snapshot from `onMutate`.
 */
export function useDeleteKnowledge(filters?: KnowledgeListFilters) {
  const qc = useQueryClient();
  const listKey = knowledgeKeys.list(filters);

  return useMutation({
    mutationFn: (id: string) => deleteKnowledge(id),
    onMutate: async (id) => {
      await qc.cancelQueries({ queryKey: listKey });
      const previous = qc.getQueryData<CursorPage<KnowledgeEntry>>(listKey);
      if (previous) {
        qc.setQueryData<CursorPage<KnowledgeEntry>>(listKey, {
          ...previous,
          items: previous.items.filter((e) => e.id !== id),
        });
      }
      return { previous };
    },
    onError: (_err, _id, context) => {
      if (context?.previous) {
        qc.setQueryData(listKey, context.previous);
      }
    },
    onSettled: () => {
      qc.invalidateQueries({ queryKey: knowledgeKeys.lists() });
      qc.invalidateQueries({ queryKey: knowledgeKeys.stats() });
    },
  });
}

/**
 * Update knowledge status with optimistic update.
 */
export function useUpdateKnowledgeStatus(filters?: KnowledgeListFilters) {
  const qc = useQueryClient();
  const listKey = knowledgeKeys.list(filters);

  return useMutation({
    mutationFn: ({
      id,
      status,
      reviewNotes,
    }: {
      id: string;
      status: KnowledgeStatus;
      reviewNotes?: string;
    }) => updateKnowledgeStatus(id, status, reviewNotes),
    onMutate: async ({ id, status }) => {
      await qc.cancelQueries({ queryKey: listKey });
      const previous = qc.getQueryData<CursorPage<KnowledgeEntry>>(listKey);
      if (previous) {
        qc.setQueryData<CursorPage<KnowledgeEntry>>(listKey, {
          ...previous,
          items: previous.items.map((e) =>
            e.id === id ? { ...e, status } : e,
          ),
        });
      }
      return { previous };
    },
    onError: (_err, _vars, context) => {
      if (context?.previous) {
        qc.setQueryData(listKey, context.previous);
      }
    },
    onSettled: () => {
      qc.invalidateQueries({ queryKey: knowledgeKeys.lists() });
      qc.invalidateQueries({ queryKey: knowledgeKeys.stats() });
    },
  });
}
