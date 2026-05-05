"use client";

// TanStack hooks around the binding-suggestions + /edits surface:
//
//  - `useSuggestBindings` — Glossary term → ranked property
//    candidates.
//  - `useSuggestTerms` — one property → top-N candidate glossary
//    terms (inline "Link to existing term" dropdown).
//  - `useApplyBindingEdits` — fires `OntologyEditOp::BindPropertyTo*`
//    via `/edits` and invalidates the ontology detail cache.

import {
  useMutation,
  useQueryClient,
  type UseMutationOptions,
} from "@tanstack/react-query";

import {
  suggestGlossaryBindings,
  suggestTermsForProperty,
  type SuggestBindingsRequest,
  type SuggestBindingsResponse,
  type SuggestTermsResponse,
  type OwnerKind,
  type BindingPolicy,
} from "@/lib/api/binding-suggestions";
import {
  submitOntologyEdits,
  type EditOntologyRequest,
  type OntologyEditReceipt,
} from "@/lib/api/edit-ops";

import { workspaceOntologyKeys } from "./use-workspace-ontology";

// ---------------------------------------------------------------------------
// Term → property candidates
// ---------------------------------------------------------------------------

export function useSuggestBindings(
  ontologyId: string,
  options?: UseMutationOptions<
    SuggestBindingsResponse,
    Error,
    SuggestBindingsRequest
  >,
) {
  return useMutation<SuggestBindingsResponse, Error, SuggestBindingsRequest>({
    mutationFn: (body) => suggestGlossaryBindings(ontologyId, body),
    ...options,
  });
}

// ---------------------------------------------------------------------------
// Property → term candidates
// ---------------------------------------------------------------------------

export interface SuggestTermsVariables {
  ownerKind: OwnerKind;
  ownerTypeId: string;
  propertyId: string;
  policy?: BindingPolicy;
}

export function useSuggestTerms(
  ontologyId: string,
  options?: UseMutationOptions<SuggestTermsResponse, Error, SuggestTermsVariables>,
) {
  return useMutation<SuggestTermsResponse, Error, SuggestTermsVariables>({
    mutationFn: ({ ownerKind, ownerTypeId, propertyId, policy }) =>
      suggestTermsForProperty(
        ontologyId,
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
  ontologyId: string,
  options?: UseMutationOptions<OntologyEditReceipt, Error, EditOntologyRequest>,
) {
  const queryClient = useQueryClient();
  const { onSuccess, ...rest } = options ?? {};
  return useMutation<OntologyEditReceipt, Error, EditOntologyRequest>({
    ...rest,
    mutationFn: (body) => submitOntologyEdits(ontologyId, body),
    onSuccess: (...args) => {
      queryClient.invalidateQueries({
        queryKey: workspaceOntologyKeys.all,
      });
      onSuccess?.(...args);
    },
  });
}
