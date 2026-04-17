"use client";

import {
  useMutation,
  useQuery,
  useQueryClient,
  type UseQueryOptions,
} from "@tanstack/react-query";
import {
  createProject,
  deleteProject,
  getProject,
  listProjects,
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
