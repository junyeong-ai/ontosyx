"use client";

// Phase 4.5 — TanStack hooks around the binding-suggestions +
// /edits surface. Three mutations:
//
//  - `useSuggestBindings` — Glossary term (existing or draft) →
//    ranked property candidates.
//  - `useSuggestTerms` — one property → top-N candidate glossary
//    terms (inline "Link to existing term" dropdown).
//  - `useApplyBindingEdits` — fires `OntologyEditOp::BindPropertyTo*`
//    via `/edits`. Invalidates the ontology detail key so the UI
//    reloads with the new property.glossary_term_id pointer.

import {
  useMutation,
  useQueryClient,
  type UseMutationOptions,
} from "@tanstack/react-query";

import {
  applyOntologyEdits,
  suggestGlossaryBindings,
  suggestTermsForProperty,
  type OntologyEditReceipt,
  type EditOntologyRequest,
  type SuggestBindingsRequest,
  type SuggestBindingsResponse,
  type SuggestTermsResponse,
  type OwnerKind,
  type BindingPolicyBody,
} from "@/lib/api/binding-suggestions";

import { ontologiesKeys } from "./use-ontologies";

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
  policy?: BindingPolicyBody;
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
    mutationFn: (body) => applyOntologyEdits(ontologyId, body),
    onSuccess: (...args) => {
      queryClient.invalidateQueries({
        queryKey: ontologiesKeys.detail(ontologyId),
      });
      onSuccess?.(...args);
    },
  });
}
