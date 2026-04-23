"use client";

import {
  useMutation,
  useQuery,
  useQueryClient,
  type UseMutationOptions,
} from "@tanstack/react-query";
import {
  decideStaleProposal,
  getQualityBaseline,
  getQualityMetrics,
  listShaclFailures,
  listStaleProposals,
  listStaleTypes,
  listTypeCandidates,
  type MetricWindow,
  type QualityBaseline,
  type TypeCandidate,
} from "@/lib/api/quality";
import type { StaleConceptProposal } from "@/types/api";

export const qualityKeys = {
  all: ["quality-signals"] as const,
  metrics: (window: MetricWindow) =>
    [...qualityKeys.all, "metrics", window] as const,
  baseline: () => [...qualityKeys.all, "baseline"] as const,
  shaclFailures: (window: MetricWindow) =>
    [...qualityKeys.all, "shacl-failures", window] as const,
  stale: (days: number) => [...qualityKeys.all, "stale", days] as const,
  staleProposals: (includeDecided: boolean) =>
    [...qualityKeys.all, "stale-proposals", includeDecided] as const,
};

export function useQualityMetrics(window: MetricWindow = "7d") {
  return useQuery({
    queryKey: qualityKeys.metrics(window),
    queryFn: () => getQualityMetrics(window),
  });
}

/// Fetch the workspace's adaptive-threshold snapshot. Returns
/// `null` until the daily cron runs; callers that render banners
/// merge this with their hardcoded prior — see
/// `lib/quality/alerts.ts::resolveThresholds`.
///
/// Refetched at `staleTime: 5min` — the underlying row changes
/// at most once per 24h, so aggressive cache + background refetch
/// on tab focus keeps the UI responsive without hammering the API.
export function useQualityBaseline() {
  return useQuery<QualityBaseline | null>({
    queryKey: qualityKeys.baseline(),
    queryFn: () => getQualityBaseline(),
    staleTime: 5 * 60 * 1000,
  });
}

export function useShaclFailures(window: MetricWindow = "7d") {
  return useQuery({
    queryKey: qualityKeys.shaclFailures(window),
    queryFn: () => listShaclFailures(window),
  });
}

export function useStaleTypes(staleAfterDays = 180) {
  return useQuery({
    queryKey: qualityKeys.stale(staleAfterDays),
    queryFn: () => listStaleTypes(staleAfterDays),
  });
}

export function useStaleProposals(includeDecided = false) {
  return useQuery({
    queryKey: qualityKeys.staleProposals(includeDecided),
    queryFn: () => listStaleProposals(includeDecided),
  });
}

export interface DecideStaleVariables {
  id: string;
  decision: "approved" | "dismissed";
  reason?: string;
}

/**
 * Fetch which ontologies in the workspace carry a node/edge type
 * with the given logical id. Feeds the Phase 4.7 auto-deprecate
 * flow on the stale-approval page — enabled is gated so the lookup
 * only runs when the page explicitly opts in (after approve).
 */
export function useTypeCandidates(
  logicalId: string | null,
  kind: string | null,
  options?: { enabled?: boolean },
) {
  return useQuery<TypeCandidate[]>({
    queryKey: [
      ...qualityKeys.all,
      "type-candidates",
      logicalId ?? "",
      kind ?? "",
    ] as const,
    queryFn: () => listTypeCandidates(logicalId!, kind!),
    enabled:
      (options?.enabled ?? true) && Boolean(logicalId) && Boolean(kind),
  });
}

export function useDecideStaleProposal(
  options?: UseMutationOptions<
    StaleConceptProposal,
    Error,
    DecideStaleVariables
  >,
) {
  const queryClient = useQueryClient();
  const { onSuccess, ...rest } = options ?? {};
  return useMutation<StaleConceptProposal, Error, DecideStaleVariables>({
    ...rest,
    mutationFn: ({ id, decision, reason }) =>
      decideStaleProposal(id, decision, reason),
    onSuccess: (...args) => {
      // Invalidate both proposal lists (pending + include-decided)
      // since a decision moves the row between them.
      queryClient.invalidateQueries({
        queryKey: [...qualityKeys.all, "stale-proposals"],
      });
      onSuccess?.(...args);
    },
  });
}
