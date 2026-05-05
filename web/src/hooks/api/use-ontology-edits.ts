"use client";

import { useMutation, useQueryClient } from "@tanstack/react-query";

import {
  submitOntologyEdits,
  type OntologyEditOp,
  type OntologyEditReceipt,
} from "@/lib/api/edit-ops";
import { workspaceOntologyKeys } from "./use-workspace-ontology";

/**
 * Apply a batch of `OntologyEditOp` ops against the named ontology
 * and refresh the cache so dependent UI (the detail view, vocabulary
 * lists, navigation panels) re-renders against the new committed
 * version.
 *
 * Generic across collections: every vocabulary CRUD path picks an
 * `OntologyEditOp` variant, sets `expected_version`, and POSTs.
 * Centralising the cache-invalidation rule keeps the per-call sites
 * trivial.
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
      // Workspace × ontology is 1:1; invalidating the singleton key
      // refreshes both the identity row and the version tag in one
      // pass.
      qc.invalidateQueries({ queryKey: workspaceOntologyKeys.all });
    },
  });
}
