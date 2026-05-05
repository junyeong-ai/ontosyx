"use client";

import {
  useInfiniteQuery,
  useMutation,
  useQuery,
  useQueryClient,
  type UseQueryOptions,
} from "@tanstack/react-query";
import {
  bulkReviewKnowledge,
  createKnowledge,
  deleteKnowledge,
  knowledgeStats,
  listKnowledge,
  updateKnowledgeStatus,
} from "@/lib/api/knowledge";
import { useOptimisticMutation } from "@/hooks/api/use-optimistic-mutation";
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
 * the round-trip before removing the row feels laggy. On failure the
 * shared `useOptimisticMutation` rollback runs against the cancelled
 * snapshot.
 */
export function useDeleteKnowledge(filters?: KnowledgeListFilters) {
  return useOptimisticMutation<string, void>({
    mutationFn: (id) => deleteKnowledge(id),
    queryKeys: [knowledgeKeys.list(filters), knowledgeKeys.stats()],
    optimisticUpdate: (prev, id) => {
      if (!isKnowledgePage(prev)) return prev;
      // Knowledge stats cache holds a different shape — the type
      // guard rejects it, so the stats key flows through untouched
      // and `onSettled` invalidates both.
      return { ...prev, items: prev.items.filter((e) => e.id !== id) };
    },
  });
}

/**
 * Update knowledge status with optimistic update — flips the status
 * field in the visible row immediately, rolls back on server error.
 */
export function useUpdateKnowledgeStatus(filters?: KnowledgeListFilters) {
  type Vars = { id: string; status: KnowledgeStatus; reviewNotes?: string };
  return useOptimisticMutation<Vars, void>({
    mutationFn: ({ id, status, reviewNotes }) =>
      updateKnowledgeStatus(id, status, reviewNotes),
    queryKeys: [knowledgeKeys.list(filters), knowledgeKeys.stats()],
    optimisticUpdate: (prev, { id, status }) => {
      if (!isKnowledgePage(prev)) return prev;
      return {
        ...prev,
        items: prev.items.map((e) => (e.id === id ? { ...e, status } : e)),
      };
    },
  });
}

/**
 * Bulk-review a batch of entries (steward workflow — "approve
 * everything in this filter view" / "deprecate every stale row").
 * The optimistic transform flips status on every selected id at
 * once; rollback restores the per-key snapshot atomically.
 *
 * The BE `POST /api/knowledge/bulk-review` accepts up to 100 ids
 * per call (the gate fires the typed `bulk_limit_exceeded` code),
 * so the FE caps the batch before issuing the request — splitting
 * into multiple calls is the caller's responsibility.
 */
export function useBulkReviewKnowledge(filters?: KnowledgeListFilters) {
  type Vars = { ids: string[]; status: KnowledgeStatus; reviewNotes?: string };
  return useOptimisticMutation<Vars, { reviewed: number }>({
    mutationFn: ({ ids, status, reviewNotes }) =>
      bulkReviewKnowledge(ids, status, reviewNotes),
    queryKeys: [knowledgeKeys.list(filters), knowledgeKeys.stats()],
    optimisticUpdate: (prev, { ids, status }) => {
      if (!isKnowledgePage(prev)) return prev;
      const idSet = new Set(ids);
      return {
        ...prev,
        items: prev.items.map((e) =>
          idSet.has(e.id) ? { ...e, status } : e,
        ),
      };
    },
  });
}

/**
 * Runtime type guard the optimistic-mutation transforms use to
 * narrow `unknown` to the knowledge list-page shape. The `useOptimisticMutation`
 * hook passes the same callback to multiple keys (list + stats) so
 * a guard that rejects non-list shapes lets the stats key flow
 * through unchanged without a separate transform.
 */
function isKnowledgePage(value: unknown): value is CursorPage<KnowledgeEntry> {
  return (
    typeof value === "object" &&
    value !== null &&
    "items" in value &&
    Array.isArray((value as { items: unknown }).items)
  );
}
