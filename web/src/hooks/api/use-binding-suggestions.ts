"use client";

// TanStack hooks around the binding-suggestions + /edits surface:
//
//  - `useSuggestBindings` — concept label → ranked property
//    candidates.
//  - `useSuggestConcepts` — one property → top-N candidate
//    concepts (inline "Link to concept" dropdown).
//  - `useApplyBindingEdits` — fires `OntologyEditOp::BindPropertyTo*`
//    via `/edits` and invalidates the ontology detail cache.

import {
  useMutation,
  useQueryClient,
  type UseMutationOptions,
} from "@tanstack/react-query";

import {
  suggestConceptPropertyBindings,
  suggestConceptsForProperty,
  type SuggestBindingsRequest,
  type SuggestBindingsResponse,
  type SuggestConceptsResponse,
  type OwnerKind,
  type BindingPolicy,
} from "@/lib/api/binding-suggestions";
import {
  isOntologyEditReceipt,
  submitOntologyEdits,
  type EditOntologyRequest,
  type OntologyEditReceipt,
} from "@/lib/api/edit-ops";

import { workspaceOntologyKeys } from "./use-workspace-ontology";

// ---------------------------------------------------------------------------
// Concept label → property candidates
// ---------------------------------------------------------------------------

export function useSuggestBindings(
  _ontologyId: string,
  options?: UseMutationOptions<
    SuggestBindingsResponse,
    Error,
    SuggestBindingsRequest
  >,
) {
  return useMutation<SuggestBindingsResponse, Error, SuggestBindingsRequest>({
    mutationFn: (body) => suggestConceptPropertyBindings("workspace", body),
    ...options,
  });
}

// ---------------------------------------------------------------------------
// Property → concept candidates
// ---------------------------------------------------------------------------

export interface SuggestConceptsVariables {
  ownerKind: OwnerKind;
  ownerTypeId: string;
  propertyId: string;
  policy?: BindingPolicy;
}

export function useSuggestConcepts(
  _ontologyId: string,
  options?: UseMutationOptions<SuggestConceptsResponse, Error, SuggestConceptsVariables>,
) {
  return useMutation<SuggestConceptsResponse, Error, SuggestConceptsVariables>({
    mutationFn: ({ ownerKind, ownerTypeId, propertyId, policy }) =>
      suggestConceptsForProperty(
        "workspace",
        ownerKind,
        ownerTypeId,
        propertyId,
        policy,
      ),
    ...options,
  });
}

// ---------------------------------------------------------------------------
// Apply binding edits
// ---------------------------------------------------------------------------

export function useApplyBindingEdits(
  _ontologyId: string,
  options?: UseMutationOptions<OntologyEditReceipt, Error, EditOntologyRequest>,
) {
  const queryClient = useQueryClient();
  const { onSuccess, ...rest } = options ?? {};
  return useMutation<OntologyEditReceipt, Error, EditOntologyRequest>({
    ...rest,
    mutationFn: async (body) => {
      const response = await submitOntologyEdits("workspace", body);
      if (!isOntologyEditReceipt(response)) {
        throw new Error("Binding edits must be submitted as a commit, not a dry run");
      }
      return response;
    },
    onSuccess: (...args) => {
      queryClient.invalidateQueries({
        queryKey: workspaceOntologyKeys.all,
      });
      onSuccess?.(...args);
    },
  });
}
