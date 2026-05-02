"use client";

import { useQuery } from "@tanstack/react-query";

import { listMembers } from "@/lib/api/workspaces";
import type { WorkspaceMember } from "@/types/workspace";

export const workspaceMembersKeys = {
  all: ["workspace-members"] as const,
  list: (workspaceId: string) =>
    [...workspaceMembersKeys.all, workspaceId] as const,
};

/**
 * Fetch the active workspace's member roster. Cached for 60s so
 * collaboration surfaces (presence avatars, lock indicators)
 * resolve names / emails / roles without a request per render.
 */
export function useWorkspaceMembers(workspaceId: string | null | undefined) {
  return useQuery({
    queryKey: workspaceMembersKeys.list(workspaceId ?? ""),
    queryFn: () => listMembers(workspaceId!),
    enabled: !!workspaceId,
    staleTime: 60_000,
  });
}

/** Optional convenience — `user_id → member` map for O(1) lookup. */
export function membersByUserId(
  members: WorkspaceMember[] | undefined,
): Map<string, WorkspaceMember> {
  const map = new Map<string, WorkspaceMember>();
  if (!members) return map;
  for (const m of members) map.set(m.user_id, m);
  return map;
}
