"use client";

import { useQuery } from "@tanstack/react-query";

import { request } from "@/lib/api/client";
import type { components } from "@/types/api.generated";

export type DiagnosticMessage = components["schemas"]["DiagnosticMessage"];

export const ontologyValidationKeys = {
  all: ["ontology-validation"] as const,
  detail: (ontologyId: string) =>
    [...ontologyValidationKeys.all, ontologyId] as const,
};

async function fetchOntologyValidation(
  ontologyId: string,
): Promise<DiagnosticMessage[]> {
  return request<DiagnosticMessage[]>(
    `/ontologies/${encodeURIComponent(ontologyId)}/validate`,
  );
}

/**
 * Fetch the structural validation diagnostics for the named
 * ontology's current version. Cached aggressively — diagnostics
 * derive from the committed IR snapshot, so they only change on
 * commit; admin-form mutations should invalidate the key on
 * success.
 */
export function useOntologyValidation(ontologyId: string | null | undefined) {
  return useQuery<DiagnosticMessage[]>({
    queryKey: ontologyValidationKeys.detail(ontologyId ?? "__none__"),
    queryFn: () => fetchOntologyValidation(ontologyId!),
    enabled: !!ontologyId,
    staleTime: 5 * 60 * 1000,
  });
}

/**
 * Predicate: keeps diagnostics whose params contain `expected` at
 * the given key. Used by per-entity admin forms to slice the full
 * validation vector down to "issues that pertain to this rule" /
 * "issues that pertain to this mapping". Exact equality on the
 * stringified value — the validator emits ids verbatim.
 */
export function diagnosticHasParam(
  diag: DiagnosticMessage,
  key: string,
  expected: string,
): boolean {
  const value = diag.params?.[key];
  return typeof value === "string" && value === expected;
}
