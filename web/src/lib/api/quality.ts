import type {
  QualityMetricsReport,
  ShaclFailureCount,
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
