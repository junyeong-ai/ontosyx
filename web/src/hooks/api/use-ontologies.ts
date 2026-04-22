"use client";

import {
  useInfiniteQuery,
  useMutation,
  useQuery,
  useQueryClient,
  type UseQueryOptions,
} from "@tanstack/react-query";
import {
  adoptGraph,
  getOntologyDetail,
  listOntologies,
  reindexSchema,
} from "@/lib/api/ontology";
import type {
  CursorPage,
  OntologyDetail,
  OntologyIR,
  OntologyListItem,
} from "@/types/api";

// ---------------------------------------------------------------------------
// Query keys — TanStack convention (hierarchical factory)
// ---------------------------------------------------------------------------

export const ontologiesKeys = {
  all: ["ontologies"] as const,
  lists: () => [...ontologiesKeys.all, "list"] as const,
  list: (params?: { limit?: number }) =>
    [...ontologiesKeys.lists(), params ?? {}] as const,
  details: () => [...ontologiesKeys.all, "detail"] as const,
  detail: (id: string) => [...ontologiesKeys.details(), id] as const,
};

// ---------------------------------------------------------------------------
// Queries
// ---------------------------------------------------------------------------

/**
 * List ontologies (first page).
 *
 * Why `useQuery` over `useInfiniteQuery`: ContextSelector + latest-ontology
 * flows only consume the first page (typically `limit: 1` or `limit: 100`),
 * so pagination cursors aren't needed. Pages that need "load more" should
 * use `useOntologiesInfinite` below.
 */
export function useOntologies(
  params?: { limit?: number },
  options?: Omit<
    UseQueryOptions<CursorPage<OntologyListItem>>,
    "queryKey" | "queryFn"
  >,
) {
  return useQuery({
    queryKey: ontologiesKeys.list(params),
    queryFn: () => listOntologies(params),
    ...options,
  });
}

/**
 * Fetch one ontology's detail (identity + hydrated IR). Pass `null` or
 * `undefined` to disable — the hook parks in idle state until an id
 * arrives.
 */
export function useOntologyDetail(
  id: string | null | undefined,
  options?: Omit<UseQueryOptions<OntologyDetail>, "queryKey" | "queryFn" | "enabled">,
) {
  return useQuery({
    queryKey: ontologiesKeys.detail(id ?? ""),
    queryFn: () => getOntologyDetail(id!),
    enabled: Boolean(id),
    ...options,
  });
}

/**
 * Infinite list — use for UIs with a "Load more" button.
 *
 * Why `useInfiniteQuery`: cursor-paginated endpoints return
 * `{ items, next_cursor }`. TanStack handles page concatenation and keeps
 * previously loaded items in cache across `fetchNextPage` calls.
 */
export function useOntologiesInfinite(limit = 50) {
  return useInfiniteQuery({
    queryKey: [...ontologiesKeys.lists(), { limit, infinite: true }],
    queryFn: ({ pageParam }) =>
      listOntologies({ limit, cursor: pageParam as string | undefined }),
    initialPageParam: undefined as string | undefined,
    getNextPageParam: (last) => last.next_cursor ?? undefined,
  });
}

// ---------------------------------------------------------------------------
// Mutations
// ---------------------------------------------------------------------------

/**
 * Adopt the live graph as a new ontology. On success, invalidates
 * `ontologiesKeys.lists()` so selectors re-fetch the updated list.
 */
export function useAdoptGraph() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (req: { name?: string; save?: boolean }) =>
      adoptGraph(req.name, req.save) as Promise<OntologyIR>,
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ontologiesKeys.lists() });
    },
  });
}

/**
 * Reindex a saved ontology's label RAG.
 * Invalidates the single detail entry to reflect any updated
 * `nodes_indexed`/meta counts.
 */
export function useReindexSchema() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => reindexSchema(id),
    onSuccess: (_data, id) => {
      qc.invalidateQueries({ queryKey: ontologiesKeys.detail(id) });
      qc.invalidateQueries({ queryKey: ontologiesKeys.lists() });
    },
  });
}
