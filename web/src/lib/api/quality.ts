import type {
  QualityMetricsReport,
  ShaclFailureCount,
  StaleConceptProposal,
  StaleTypeEntry,
} from "@/types/api";
import { request } from "./client";

export type MetricWindow = "7d" | "30d" | "90d";

// ---------------------------------------------------------------------------
// Adaptive thresholds — consumer types
//
// The backend cron writes `workspace_quality_baseline` rows with a
// per-metric `{ median, mad, warn, critical }` bundle. The banner
// reads these at render time; when the baseline is absent or the
// sample is too small (MIN_SAMPLE_SIZE), the FE falls back to the
// hardcoded prior in `lib/quality/alerts.ts`.
// ---------------------------------------------------------------------------

/// Per-metric adaptive threshold bundle. All four fields live in
/// the same unit as the metric itself (proportions are in `[0, 1]`).
export interface AdaptiveThreshold {
  median: number;
  mad: number;
  warn: number;
  critical: number;
}

/// Workspace-level baseline snapshot written by the daily cron.
/// `thresholds` is keyed by metric name (`shacl_pass_rate`,
/// `query_reproducibility`, etc.) so new metrics land without a
/// schema change. `null` = cron hasn't run yet; sample < minimum
/// = the cron did run but there isn't enough signal to trust.
export interface QualityBaseline {
  workspace_id: string;
  window: MetricWindow;
  sample_size: number;
  thresholds: Record<string, AdaptiveThreshold>;
  computed_at: string;
}

export async function getQualityMetrics(
  window: MetricWindow = "7d",
): Promise<QualityMetricsReport> {
  return request<QualityMetricsReport>(`/quality/metrics?window=${window}`);
}

export async function getQualityBaseline(): Promise<QualityBaseline | null> {
  return request<QualityBaseline | null>(`/quality/baseline`);
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

// Given a stale type proposal, find which ontologies in the
// workspace carry that type. Drives the approval UI's
// auto-deprecate dispatch / target-picker.
export interface TypeCandidate {
  ontology_id: string;
  ontology_name: string;
  current_version: string;
  label: string;
  deprecated_at?: string | null;
}

export async function listTypeCandidates(
  logicalId: string,
  kind: string,
): Promise<TypeCandidate[]> {
  const qs = new URLSearchParams({
    logical_id: logicalId,
    kind,
  });
  return request<TypeCandidate[]>(`/ontologies/type-candidates?${qs}`);
}
