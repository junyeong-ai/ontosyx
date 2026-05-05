"use client";

// Approval admin surface — list query + single / bulk review
// mutations on top of `useOptimisticMutation`. Mirrors the
// knowledge / stale-proposal hook shape so the surfaces share
// one mental model for "list + review + bulk-review".

import { useQuery } from "@tanstack/react-query";

import {
  bulkReviewApprovals,
  listApprovals,
  reviewApproval,
  type ApprovalRequest,
} from "@/lib/api/approvals";
import { useOptimisticMutation } from "@/hooks/api/use-optimistic-mutation";

export const approvalsKeys = {
  all: ["approvals"] as const,
  list: () => [...approvalsKeys.all, "list"] as const,
};

export function useApprovals() {
  return useQuery({
    queryKey: approvalsKeys.list(),
    queryFn: listApprovals,
  });
}

/**
 * Single-id review with optimistic transition. Flips the row's
 * `status` immediately so the UI moves it to the resolved
 * section without waiting on the round-trip; rollback restores
 * the previous status on server error.
 */
export function useReviewApproval() {
  type Vars = { id: string; approved: boolean; note?: string };
  return useOptimisticMutation<Vars, { status: string }>({
    mutationFn: ({ id, approved, note }) => reviewApproval(id, approved, note),
    queryKeys: [approvalsKeys.list()],
    optimisticUpdate: (prev, { id, approved }) => {
      if (!isApprovalList(prev)) return prev;
      const next = approved ? "approved" : "rejected";
      return prev.map((a) =>
        a.id === id && a.status === "pending" ? { ...a, status: next } : a,
      );
    },
  });
}

/**
 * Bulk-review every selected approval in one round-trip. Same
 * optimistic transform as the single-id path applied across the
 * cohort; the BE caps `ids.len()` at 100 (the typed
 * `bulk_limit_exceeded` gate), the FE caller splits when it has
 * to.
 */
export function useBulkReviewApprovals() {
  type Vars = { ids: string[]; approved: boolean; note?: string };
  return useOptimisticMutation<Vars, { reviewed: number }>({
    mutationFn: ({ ids, approved, note }) =>
      bulkReviewApprovals(ids, approved, note),
    queryKeys: [approvalsKeys.list()],
    optimisticUpdate: (prev, { ids, approved }) => {
      if (!isApprovalList(prev)) return prev;
      const next = approved ? "approved" : "rejected";
      const idSet = new Set(ids);
      return prev.map((a) =>
        idSet.has(a.id) && a.status === "pending"
          ? { ...a, status: next }
          : a,
      );
    },
  });
}

function isApprovalList(value: unknown): value is ApprovalRequest[] {
  return Array.isArray(value);
}
