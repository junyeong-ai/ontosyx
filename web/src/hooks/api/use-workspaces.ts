"use client";

import {
  useMutation,
  useQuery,
  useQueryClient,
  type UseQueryOptions,
} from "@tanstack/react-query";
import {
  createWorkspace,
  deleteWorkspace,
  getWorkspace,
  listWorkspaces,
  updateWorkspace,
} from "@/lib/api/workspaces";
import type {
  CreateWorkspaceRequest,
  UpdateWorkspaceRequest,
  Workspace,
  WorkspaceSummary,
} from "@/types/workspace";

// ---------------------------------------------------------------------------
// Query keys
// ---------------------------------------------------------------------------

export const workspacesKeys = {
  all: ["workspaces"] as const,
  lists: () => [...workspacesKeys.all, "list"] as const,
  list: () => [...workspacesKeys.lists(), {}] as const,
  details: () => [...workspacesKeys.all, "detail"] as const,
  detail: (id: string) => [...workspacesKeys.details(), id] as const,
};

// ---------------------------------------------------------------------------
// Queries
// ---------------------------------------------------------------------------

/**
 * List workspaces the current user is a member of.
 *
 * Why plain `useQuery` (no infinite pagination): the workspace switcher shows
 * every workspace the user belongs to — typically well under 50. If a
 * power-user scenario ever drives the count higher, swap for
 * `useInfiniteQuery` without touching call sites.
 */
export function useWorkspaces(
  options?: Omit<
    UseQueryOptions<WorkspaceSummary[]>,
    "queryKey" | "queryFn"
  >,
) {
  return useQuery({
    queryKey: workspacesKeys.list(),
    queryFn: listWorkspaces,
    staleTime: 30_000,
    ...options,
  });
}

export function useWorkspace(
  id: string | null | undefined,
  options?: Omit<
    UseQueryOptions<Workspace>,
    "queryKey" | "queryFn" | "enabled"
  >,
) {
  return useQuery({
    queryKey: workspacesKeys.detail(id ?? ""),
    queryFn: () => getWorkspace(id as string),
    enabled: !!id,
    ...options,
  });
}

// ---------------------------------------------------------------------------
// Mutations
// ---------------------------------------------------------------------------

export function useCreateWorkspace() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (req: CreateWorkspaceRequest) => createWorkspace(req),
    onSuccess: (ws) => {
      qc.invalidateQueries({ queryKey: workspacesKeys.lists() });
      qc.setQueryData(workspacesKeys.detail(ws.id), ws);
    },
  });
}

export function useUpdateWorkspace() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, req }: { id: string; req: UpdateWorkspaceRequest }) =>
      updateWorkspace(id, req),
    onSuccess: (ws) => {
      qc.invalidateQueries({ queryKey: workspacesKeys.lists() });
      qc.setQueryData(workspacesKeys.detail(ws.id), ws);
    },
  });
}

export function useDeleteWorkspace() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => deleteWorkspace(id),
    onSuccess: (_data, id) => {
      qc.invalidateQueries({ queryKey: workspacesKeys.lists() });
      qc.removeQueries({ queryKey: workspacesKeys.detail(id) });
    },
  });
}
