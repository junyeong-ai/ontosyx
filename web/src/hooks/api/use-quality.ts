"use client";

import { useQuery } from "@tanstack/react-query";
import {
  getQualityMetrics,
  listShaclFailures,
  listStaleTypes,
  type MetricWindow,
} from "@/lib/api/quality";

export const qualityKeys = {
  all: ["quality-signals"] as const,
  metrics: (window: MetricWindow) =>
    [...qualityKeys.all, "metrics", window] as const,
  shaclFailures: (window: MetricWindow) =>
    [...qualityKeys.all, "shacl-failures", window] as const,
  stale: (days: number) => [...qualityKeys.all, "stale", days] as const,
};

export function useQualityMetrics(window: MetricWindow = "7d") {
  return useQuery({
    queryKey: qualityKeys.metrics(window),
    queryFn: () => getQualityMetrics(window),
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
