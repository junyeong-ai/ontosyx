"use client";

import { useQuery } from "@tanstack/react-query";

import { getExecution } from "@/lib/api/queries";
import type { QueryExecution } from "@/types/api";

export const executionsKeys = {
  all: ["executions"] as const,
  detail: (id: string) => [...executionsKeys.all, "detail", id] as const,
};

/**
 * Fetch a single persisted query execution by id.
 *
 * The execution row is the canonical source for rendering metadata
 * (provenance, compiled target, query bindings) that the agent's
 * tool result intentionally omits — see ox-agent's tool-result
 * contract.
 *
 * Pass `null` to park the query in idle without firing a request
 * (call-sites that conditionally have an id pre-resolved).
 */
export function useExecution(executionId: string | null) {
  return useQuery<QueryExecution>({
    queryKey: executionsKeys.detail(executionId ?? ""),
    queryFn: () => getExecution(executionId!),
    enabled: !!executionId,
  });
}
