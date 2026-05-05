"use client";

import {
  useMutation,
  useQuery,
  useQueryClient,
  type UseQueryOptions,
} from "@tanstack/react-query";
import {
  createOntologyDraft,
  deferScopeTables,
  deleteOntologyDraft,
  getOntologyDraft,
  includeScopeTables,
  listOntologyDrafts,
  type DeferScopeTablesRequest,
  type IncludeScopeTablesRequest,
  type ScopeUpdateResponse,
} from "@/lib/api/ontology-drafts";
import type {
  CreateProjectRequest,
  CursorPage,
  OntologyDraft,
  OntologyDraftSummary,
} from "@/types/api";

// ---------------------------------------------------------------------------
// Query keys
// ---------------------------------------------------------------------------

export const ontologyDraftsKeys = {
  all: ["projects"] as const,
  lists: () => [...ontologyDraftsKeys.all, "list"] as const,
  list: (params?: { limit?: number }) =>
    [...ontologyDraftsKeys.lists(), params ?? {}] as const,
  details: () => [...ontologyDraftsKeys.all, "detail"] as const,
  detail: (id: string) => [...ontologyDraftsKeys.details(), id] as const,
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
 * touching call sites of `useOntologyDraft(id)`.
 */
export function useOntologyDrafts(
  params?: { limit?: number },
  options?: Omit<
    UseQueryOptions<CursorPage<OntologyDraftSummary>>,
    "queryKey" | "queryFn"
  >,
) {
  return useQuery({
    queryKey: ontologyDraftsKeys.list(params),
    queryFn: () => listOntologyDrafts(params),
    ...options,
  });
}

export function useOntologyDraft(
  id: string | null | undefined,
  options?: Omit<
    UseQueryOptions<OntologyDraft>,
    "queryKey" | "queryFn" | "enabled"
  >,
) {
  return useQuery({
    queryKey: ontologyDraftsKeys.detail(id ?? ""),
    queryFn: () => getOntologyDraft(id as string),
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
    mutationFn: (req: CreateProjectRequest) => createOntologyDraft(req),
    onSuccess: (project) => {
      qc.invalidateQueries({ queryKey: ontologyDraftsKeys.lists() });
      // Seed detail cache so immediate navigation to /projects/:id doesn't
      // refetch. The returned OntologyDraft is authoritative.
      qc.setQueryData(ontologyDraftsKeys.detail(project.id), project);
    },
  });
}

export function useDeleteProject() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => deleteOntologyDraft(id),
    onSuccess: (_data, id) => {
      qc.invalidateQueries({ queryKey: ontologyDraftsKeys.lists() });
      qc.removeQueries({ queryKey: ontologyDraftsKeys.detail(id) });
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
      qc.setQueryData(ontologyDraftsKeys.detail(projectId), data.project);
      qc.invalidateQueries({ queryKey: ontologyDraftsKeys.lists() });
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
      qc.setQueryData(ontologyDraftsKeys.detail(projectId), data.project);
      qc.invalidateQueries({ queryKey: ontologyDraftsKeys.lists() });
    },
  });
}
