import type {
  QualityMetricsReport,
  ShaclFailureCount,
  StaleConceptProposal,
  StaleTypeEntry,
} from "@/types/api";
import { request } from "./client";

export type MetricWindow = "7d" | "30d" | "90d";

export async function getQualityMetrics(
  window: MetricWindow = "7d",
): Promise<QualityMetricsReport> {
  return request<QualityMetricsReport>(`/quality/metrics?window=${window}`);
}

export async function listShaclFailures(
  window: MetricWindow = "7d",
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
