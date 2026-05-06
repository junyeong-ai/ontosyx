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
  CreateOntologyDraftRequest,
  CursorPage,
  OntologyDraft,
  OntologyDraftSummary,
} from "@/types/api";

// ---------------------------------------------------------------------------
// Query keys
// ---------------------------------------------------------------------------

export const ontologyDraftsKeys = {
  all: ["ontology-drafts"] as const,
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
 * List ontology drafts (first page).
 *
 * Why plain `useQuery`: the draft picker shows top-N items (default limit
 * is the backend default). Design mode rarely requires paging through more
 * than ~50 drafts. If that changes, swap for `useInfiniteQuery` without
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

export function useCreateOntologyDraft() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (req: CreateOntologyDraftRequest) => createOntologyDraft(req),
    onSuccess: (draft) => {
      qc.invalidateQueries({ queryKey: ontologyDraftsKeys.lists() });
      // Seed detail cache so immediate navigation to the draft route
      // doesn't refetch. The returned OntologyDraft is authoritative.
      qc.setQueryData(ontologyDraftsKeys.detail(draft.id), draft);
    },
  });
}

export function useDeleteOntologyDraft() {
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
 * `included`. Wraps `POST /api/ontology-drafts/:id/scope/include`.
 *
 * Per-table reclassification — does not introspect or redesign;
 * the staged-bootstrap flow's deferred entries land here for
 * one-click promotion.
 */
export function useIncludeScopeTables(ontologyDraftId: string) {
  const qc = useQueryClient();
  return useMutation<ScopeUpdateResponse, Error, IncludeScopeTablesRequest>({
    mutationFn: (req) => includeScopeTables(ontologyDraftId, req),
    onSuccess: (data) => {
      qc.setQueryData(ontologyDraftsKeys.detail(ontologyDraftId), data.draft);
      qc.invalidateQueries({ queryKey: ontologyDraftsKeys.lists() });
    },
  });
}

/**
 * Demote tables from `included` to `deferred`. Wraps
 * `POST /api/ontology-drafts/:id/scope/defer`. Backend rejects
 * with 409 if the draft's ontology still binds a NodeType to one
 * of the tables — the caller must retract those nodes first.
 */
export function useDeferScopeTables(ontologyDraftId: string) {
  const qc = useQueryClient();
  return useMutation<ScopeUpdateResponse, Error, DeferScopeTablesRequest>({
    mutationFn: (req) => deferScopeTables(ontologyDraftId, req),
    onSuccess: (data) => {
      qc.setQueryData(ontologyDraftsKeys.detail(ontologyDraftId), data.draft);
      qc.invalidateQueries({ queryKey: ontologyDraftsKeys.lists() });
    },
  });
}
