import type {
  QualityMetricsReport,
  ShaclFailureCount,
  StaleConceptProposal,
  StaleTypeEntry,
} from "@/types/api";
import type { components } from "@/types/api.generated";
import { request } from "./client";

export type MetricWindow = components["schemas"]["MetricWindow"];

// ---------------------------------------------------------------------------
// Adaptive thresholds — consumer types
//
// The backend cron writes `workspace_quality_baseline` rows with a
// per-metric `{ median, mad, warn, critical }` bundle. The banner
// reads these at render time; when the baseline is absent or the
// sample is too small (MIN_SAMPLE_SIZE), the FE falls back to the
// hardcoded prior in `lib/quality/alerts.ts`.
// ---------------------------------------------------------------------------

export interface AdaptiveThreshold {
  median: number;
  mad: number;
  warn: number;
  critical: number;
}

export type QualityBaseline = Omit<
  components["schemas"]["WorkspaceQualityBaseline"],
  "thresholds"
> & {
  thresholds: Record<string, AdaptiveThreshold>;
};

export async function getQualityMetrics(
  window: MetricWindow = "last7d",
): Promise<QualityMetricsReport> {
  return request<QualityMetricsReport>(`/quality/metrics?window=${window}`);
}

export async function getQualityBaseline(): Promise<QualityBaseline | null> {
  return request<QualityBaseline | null>(`/quality/baseline`);
}

export async function listShaclFailures(
  window: MetricWindow = "last7d",
): Promise<ShaclFailureCount[]> {
  return request<ShaclFailureCount[]>(
    `/quality/shacl-failures?window=${window}`,
  );
}

export async function listStaleTypes(
  staleAfterDays = 180,
): Promise<StaleTypeEntry[]> {
  return request<StaleTypeEntry[]>(
    `/quality/stale-types?stale_after_days=${staleAfterDays}`,
  );
}

// Stale-concept proposal approval flow — the daily cron writes the
// proposals, admins decide on each row here.

export async function listStaleProposals(
  includeDecided = false,
): Promise<StaleConceptProposal[]> {
  const qs = new URLSearchParams();
  if (includeDecided) qs.set("include_decided", "true");
  const q = qs.toString();
  return request<StaleConceptProposal[]>(
    `/quality/stale-proposals${q ? `?${q}` : ""}`,
  );
}

export async function decideStaleProposal(
  id: string,
  decision: "approved" | "dismissed",
  reason?: string,
): Promise<StaleConceptProposal> {
  return request<StaleConceptProposal>(
    `/quality/stale-proposals/${encodeURIComponent(id)}`,
    {
      method: "PATCH",
      body: JSON.stringify({ decision, reason }),
    },
  );
}

export async function bulkDecideStaleProposals(
  ids: string[],
  decision: "approved" | "dismissed",
  reason?: string,
): Promise<{ decided: number }> {
  return request<{ decided: number }>(
    "/quality/stale-proposals/bulk-decide",
    {
      method: "POST",
      body: JSON.stringify({ ids, decision, reason }),
    },
  );
}

// Given a stale type proposal, find which ontologies in the
// workspace carry that type. Drives the approval UI's
// auto-deprecate dispatch / target-picker.
export type TypeCandidate = components["schemas"]["TypeCandidate"];

export async function listTypeCandidates(
  logicalId: string,
  kind: string,
): Promise<TypeCandidate[]> {
  const qs = new URLSearchParams({
    logical_id: logicalId,
    kind,
  });
  return request<TypeCandidate[]>(`/ontology/type-candidates?${qs}`);
}
