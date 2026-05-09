"use client";

// Φ11.4b — verified-query bank query / mutation hooks.
//
// List / detail use TanStack `useQuery`. Status transitions and
// deletes ride `useOptimisticMutation` so the admin queue feels
// instant — the row's status flips (or disappears) immediately and
// rolls back atomically on server error. Promotion is a plain
// mutation because the server assigns the id and we want the real
// record back before injecting it into the cache.

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  deleteVerifiedQuery,
  getVerifiedQuery,
  listVerifiedQueries,
  promoteVerifiedQuery,
  transitionVerifiedQueryStatus,
} from "@/lib/api/verified-queries";
import { useOptimisticMutation } from "@/hooks/api/use-optimistic-mutation";
import type {
  PromoteVerifiedQueryRequest,
  VerifiedQuery,
  VerifiedQueryId,
  VerifiedQueryListResponse,
  VerifiedQueryStatus,
} from "@/types/api";

// ---------------------------------------------------------------------------
// Query keys
// ---------------------------------------------------------------------------

export interface VerifiedQueryListFilters {
  status?: VerifiedQueryStatus;
  limit?: number;
}

export const verifiedQueryKeys = {
  all: ["verified-queries"] as const,
  lists: () => [...verifiedQueryKeys.all, "list"] as const,
  list: (filters?: VerifiedQueryListFilters) =>
    [...verifiedQueryKeys.lists(), filters ?? {}] as const,
  details: () => [...verifiedQueryKeys.all, "detail"] as const,
  detail: (id: VerifiedQueryId) =>
    [...verifiedQueryKeys.details(), id] as const,
};

// ---------------------------------------------------------------------------
// Queries
// ---------------------------------------------------------------------------

export function useVerifiedQueries(filters?: VerifiedQueryListFilters) {
  return useQuery({
    queryKey: verifiedQueryKeys.list(filters),
    queryFn: () => listVerifiedQueries(filters),
  });
}

export function useVerifiedQuery(id: VerifiedQueryId | undefined) {
  return useQuery({
    queryKey: id ? verifiedQueryKeys.detail(id) : verifiedQueryKeys.all,
    queryFn: () => getVerifiedQuery(id as VerifiedQueryId),
    enabled: !!id,
  });
}

// ---------------------------------------------------------------------------
// Mutations
// ---------------------------------------------------------------------------

/**
 * Promote a verified query. Server assigns id + question_hash +
 * embedding (when the workspace has an embedder attached), so we
 * wait for the real record before invalidating the list cache —
 * no optimistic placeholder.
 */
export function usePromoteVerifiedQuery() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (req: PromoteVerifiedQueryRequest) => promoteVerifiedQuery(req),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: verifiedQueryKeys.lists() });
    },
  });
}

/**
 * Flip lifecycle status with an optimistic in-place update —
 * UnderReview → Verified, Verified → Deprecated, Stale → Verified
 * after a re-review. Rollback runs against the cancelled snapshot
 * on server error (typically OntologyDraftStaleParent surfacing
 * as a 409 — or the row was deleted by another admin in flight).
 */
export function useTransitionVerifiedQueryStatus(
  filters?: VerifiedQueryListFilters,
) {
  type Vars = { id: VerifiedQueryId; status: VerifiedQueryStatus };
  return useOptimisticMutation<Vars, VerifiedQuery>({
    mutationFn: ({ id, status }) =>
      transitionVerifiedQueryStatus(id, { status }),
    queryKeys: [verifiedQueryKeys.list(filters)],
    optimisticUpdate: (prev, { id, status }) => {
      if (!isVerifiedQueryListResponse(prev)) return prev;
      return {
        ...prev,
        rows: prev.rows.map((r) =>
          r.id === id ? { ...r, status } : r,
        ),
      };
    },
  });
}

/**
 * Hard-delete a verified-query row (admin path that purges
 * mistakenly-promoted entries). Soft-delete via
 * `transition → Deprecated` is the preferred path for retiring
 * a row that was ever valid; this hook is the destructive escape
 * hatch.
 */
export function useDeleteVerifiedQuery(filters?: VerifiedQueryListFilters) {
  return useOptimisticMutation<VerifiedQueryId, void>({
    mutationFn: (id) => deleteVerifiedQuery(id),
    queryKeys: [verifiedQueryKeys.list(filters)],
    optimisticUpdate: (prev, id) => {
      if (!isVerifiedQueryListResponse(prev)) return prev;
      return { ...prev, rows: prev.rows.filter((r) => r.id !== id) };
    },
  });
}

/**
 * Runtime guard for the list-response shape. The optimistic
 * transforms run against multiple cache keys; values that do
 * not match the list shape (detail entries, future siblings)
 * pass through untouched.
 */
function isVerifiedQueryListResponse(
  value: unknown,
): value is VerifiedQueryListResponse {
  return (
    typeof value === "object" &&
    value !== null &&
    "rows" in value &&
    Array.isArray((value as { rows: unknown }).rows)
  );
}
