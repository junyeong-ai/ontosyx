"use client";

import { useMutation, useQueryClient } from "@tanstack/react-query";

import {
  submitOntologyEdits,
  type OntologyEditOp,
  type OntologyEditReceipt,
} from "@/lib/api/edit-ops";
import { ontologiesKeys } from "./use-ontologies";

/**
 * Apply a batch of `OntologyEditOp` ops against the named ontology
 * and refresh the cache so dependent UI (the detail view, vocabulary
 * lists, navigation panels) re-renders against the new committed
 * version.
 *
 * Why a generic mutation hook (rather than per-collection hooks):
 * every Φ4 vocabulary CRUD page builds the same shape — pick the
 * variant, set `expected_version`, POST. A shared hook keeps the
 * cache-invalidation rule in one place.
 *
 * `expected_version` is optimistic-concurrency control: the caller
 * passes the version they read from. A racing edit lands first → the
 * server returns 409 and the caller refetches.
 */
export function useApplyOntologyEdits(ontologyId: string | null | undefined) {
  const qc = useQueryClient();
  return useMutation<
    OntologyEditReceipt,
    Error,
    {
      operations: OntologyEditOp[];
      expected_version: number;
      message?: string;
      dry_run?: boolean;
    }
  >({
    mutationFn: ({ operations, expected_version, message, dry_run }) => {
      if (!ontologyId) {
        return Promise.reject(new Error("Ontology id required"));
      }
      return submitOntologyEdits(ontologyId, {
        operations,
        expected_version,
        message,
        dry_run,
      });
    },
    onSuccess: () => {
      if (ontologyId) {
        qc.invalidateQueries({ queryKey: ontologiesKeys.detail(ontologyId) });
      }
      // Lists stay valid (the row's identity didn't change), but the
      // version number on it did — invalidate to surface the new tag.
      qc.invalidateQueries({ queryKey: ontologiesKeys.lists() });
    },
  });
}
