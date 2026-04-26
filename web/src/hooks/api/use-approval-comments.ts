"use client";

import {
  useMutation,
  useQuery,
  useQueryClient,
  type UseQueryOptions,
} from "@tanstack/react-query";

import {
  type ApprovalComment,
  createApprovalComment,
  listApprovalComments,
} from "@/lib/api/approvals";

// ---------------------------------------------------------------------------
// Query keys
// ---------------------------------------------------------------------------

export const approvalCommentsKeys = {
  all: ["approval-comments"] as const,
  thread: (approvalId: string) =>
    [...approvalCommentsKeys.all, approvalId] as const,
};

// ---------------------------------------------------------------------------
// Queries / mutations
// ---------------------------------------------------------------------------

/** Fetch the comment thread attached to an approval. */
export function useApprovalComments(
  approvalId: string,
  options?: Omit<UseQueryOptions<ApprovalComment[]>, "queryKey" | "queryFn">,
) {
  return useQuery({
    queryKey: approvalCommentsKeys.thread(approvalId),
    queryFn: () => listApprovalComments(approvalId),
    ...options,
  });
}

/** Append a comment to the thread.
 *
 *  Invalidates the thread query on success so any other open viewer
 *  picks the new entry up. Optimistic insert is intentionally not
 *  used: the server assigns the id and timestamp, and posting a
 *  comment on a stale view should still race-correctly. */
export function useCreateApprovalComment(approvalId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: string) => createApprovalComment(approvalId, body),
    onSuccess: () => {
      qc.invalidateQueries({
        queryKey: approvalCommentsKeys.thread(approvalId),
      });
    },
  });
}
