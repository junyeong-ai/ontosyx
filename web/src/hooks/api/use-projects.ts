"use client";

import {
  useMutation,
  useQuery,
  useQueryClient,
  type UseQueryOptions,
} from "@tanstack/react-query";
import {
  createProject,
  deferScopeTables,
  deleteProject,
  getProject,
  includeScopeTables,
  listProjects,
  type DeferScopeTablesRequest,
  type IncludeScopeTablesRequest,
  type ScopeUpdateResponse,
} from "@/lib/api/projects";
import type {
  CreateProjectRequest,
  CursorPage,
  DesignProject,
  DesignProjectSummary,
} from "@/types/api";

// ---------------------------------------------------------------------------
// Query keys
// ---------------------------------------------------------------------------

export const projectsKeys = {
  all: ["projects"] as const,
  lists: () => [...projectsKeys.all, "list"] as const,
  list: (params?: { limit?: number }) =>
    [...projectsKeys.lists(), params ?? {}] as const,
  details: () => [...projectsKeys.all, "detail"] as const,
  detail: (id: string) => [...projectsKeys.details(), id] as const,
};

// ---------------------------------------------------------------------------
// Queries
// ---------------------------------------------------------------------------

/**
 * List projects (first page).
 *
 * Why plain `useQuery`: the project picker shows top-N items (default limit
 * is the backend default). Design mode rarely requires paging through more
 * than ~50 projects. If that changes, swap for `useInfiniteQuery` without
 * touching call sites of `useProject(id)`.
 */
export function useProjects(
  params?: { limit?: number },
  options?: Omit<
    UseQueryOptions<CursorPage<DesignProjectSummary>>,
    "queryKey" | "queryFn"
  >,
) {
  return useQuery({
    queryKey: projectsKeys.list(params),
    queryFn: () => listProjects(params),
    ...options,
  });
}

export function useProject(
  id: string | null | undefined,
  options?: Omit<
    UseQueryOptions<DesignProject>,
    "queryKey" | "queryFn" | "enabled"
  >,
) {
  return useQuery({
    queryKey: projectsKeys.detail(id ?? ""),
    queryFn: () => getProject(id as string),
    enabled: !!id,
    ...options,
  });
}

// ---------------------------------------------------------------------------
// Mutations
// ---------------------------------------------------------------------------

export function useCreateProject() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (req: CreateProjectRequest) => createProject(req),
    onSuccess: (project) => {
      qc.invalidateQueries({ queryKey: projectsKeys.lists() });
      // Seed detail cache so immediate navigation to /projects/:id doesn't
      // refetch. The returned DesignProject is authoritative.
      qc.setQueryData(projectsKeys.detail(project.id), project);
    },
  });
}

export function useDeleteProject() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => deleteProject(id),
    onSuccess: (_data, id) => {
      qc.invalidateQueries({ queryKey: projectsKeys.lists() });
      qc.removeQueries({ queryKey: projectsKeys.detail(id) });
    },
  });
}

/**
 * Promote tables from `deferred` (or first-time-seen) into
 * `included`. Wraps `POST /api/projects/:id/scope/include`.
 *
 * Per-table reclassification — does not introspect or redesign;
 * the staged-bootstrap flow's deferred entries land here for
 * one-click promotion.
 */
export function useIncludeScopeTables(projectId: string) {
  const qc = useQueryClient();
  return useMutation<ScopeUpdateResponse, Error, IncludeScopeTablesRequest>({
    mutationFn: (req) => includeScopeTables(projectId, req),
    onSuccess: (data) => {
      qc.setQueryData(projectsKeys.detail(projectId), data.project);
      qc.invalidateQueries({ queryKey: projectsKeys.lists() });
    },
  });
}

/**
 * Demote tables from `included` to `deferred`. Wraps
 * `POST /api/projects/:id/scope/defer`. Backend rejects with 409 if
 * the project's ontology still binds a NodeType to one of the
 * tables — the caller must retract those nodes first.
 */
export function useDeferScopeTables(projectId: string) {
  const qc = useQueryClient();
  return useMutation<ScopeUpdateResponse, Error, DeferScopeTablesRequest>({
    mutationFn: (req) => deferScopeTables(projectId, req),
    onSuccess: (data) => {
      qc.setQueryData(projectsKeys.detail(projectId), data.project);
      qc.invalidateQueries({ queryKey: projectsKeys.lists() });
    },
  });
}
