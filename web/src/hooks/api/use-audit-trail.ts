"use client";

import { useInfiniteQuery } from "@tanstack/react-query";

import { listAuditRecords } from "@/lib/api/audit";
import type { AuditFilter } from "@/types/audit";

export const auditTrailKeys = {
  all: ["audit-trail"] as const,
  page: (filter: AuditFilter) =>
    [...auditTrailKeys.all, filter] as const,
};

/** Cursor-paginated stream of audit records. The hook returns the
 *  flat list of all loaded records via `data?.pages.flatMap(...)`
 *  plus the `fetchNextPage` / `hasNextPage` controls — UIs that
 *  want a single rolled-up view just spread `pages` and render. */
export function useAuditTrail(filter: AuditFilter, limit = 50) {
  return useInfiniteQuery({
    queryKey: auditTrailKeys.page(filter),
    queryFn: ({ pageParam }) =>
      listAuditRecords(filter, pageParam as string | undefined, limit),
    initialPageParam: undefined as string | undefined,
    getNextPageParam: (last) => last.next_cursor ?? undefined,
  });
}
