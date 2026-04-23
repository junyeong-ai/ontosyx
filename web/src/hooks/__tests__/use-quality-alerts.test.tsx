import { describe, it, expect, vi, beforeEach } from "vitest";
import { act, renderHook } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ReactNode } from "react";

// Stub the data layer so the hook's tests don't need a fetch mock.
const mockUseQualityMetrics = vi.fn();
vi.mock("@/hooks/api/use-quality", () => ({
  useQualityMetrics: (...args: unknown[]) => mockUseQualityMetrics(...args),
}));

import { useQualityAlerts } from "@/hooks/use-quality-alerts";
import type { QualityMetricsReport } from "@/types/api";

const DISMISS_KEY = "ontosyx.quality-banner.dismissed-signature.v1";

function report(overrides: Partial<QualityMetricsReport> = {}): QualityMetricsReport {
  const healthy = {
    value: 0.95,
    trend_delta: 0,
    lower_bound_95: 0.9,
    upper_bound_95: 0.99,
  };
  const staleHealthy = {
    value: 0.05,
    trend_delta: 0,
    lower_bound_95: 0.03,
    upper_bound_95: 0.08,
  };
  return {
    anchor_match_rate: healthy,
    glossary_hit_rate: healthy,
    clarification_success_rate: healthy,
    query_reproducibility: healthy,
    shacl_pass_rate: healthy,
    stale_concept_ratio: staleHealthy,
    sample_size: 100,
    window: "last7d",
    ...overrides,
  } as QualityMetricsReport;
}

// Every renderHook call gets a fresh QueryClient — keeps the hook
// tree isolated across tests. We also stub TanStack Query itself
// above, so the client here is only a parent for context.
function wrapper({ children }: { children: ReactNode }) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return <QueryClientProvider client={qc}>{children}</QueryClientProvider>;
}

describe("useQualityAlerts", () => {
  beforeEach(() => {
    mockUseQualityMetrics.mockReset();
    window.sessionStorage.removeItem(DISMISS_KEY);
  });

  it("stays quiet while the metrics query is loading", () => {
    mockUseQualityMetrics.mockReturnValue({ data: undefined, isLoading: true });
    const { result } = renderHook(() => useQualityAlerts(), { wrapper });
    expect(result.current.visible).toBe(false);
    expect(result.current.isLoading).toBe(true);
  });

  it("returns visible=false when the report is healthy", () => {
    mockUseQualityMetrics.mockReturnValue({ data: report(), isLoading: false });
    const { result } = renderHook(() => useQualityAlerts(), { wrapper });
    expect(result.current.alerts).toEqual([]);
    expect(result.current.visible).toBe(false);
  });

  it("returns visible=true when alerts are present and not dismissed", () => {
    mockUseQualityMetrics.mockReturnValue({
      data: report({
        shacl_pass_rate: {
          value: 0.5,
          trend_delta: 0,
          lower_bound_95: 0,
          upper_bound_95: 1,
        },
      }),
      isLoading: false,
    });
    const { result } = renderHook(() => useQualityAlerts(), { wrapper });
    expect(result.current.alerts).toHaveLength(1);
    expect(result.current.visible).toBe(true);
  });

  it("stays hidden after dismiss while the signature is the same", () => {
    mockUseQualityMetrics.mockReturnValue({
      data: report({
        shacl_pass_rate: {
          value: 0.5,
          trend_delta: 0,
          lower_bound_95: 0,
          upper_bound_95: 1,
        },
      }),
      isLoading: false,
    });
    const { result } = renderHook(() => useQualityAlerts(), { wrapper });
    expect(result.current.visible).toBe(true);
    act(() => {
      result.current.dismiss();
    });
    expect(result.current.visible).toBe(false);
    // Persisted so a fresh render (new hook instance, same
    // sessionStorage) also reads it as dismissed.
    expect(window.sessionStorage.getItem(DISMISS_KEY)).not.toBeNull();
  });

  it("re-surfaces when a new metric starts alerting (signature flips)", () => {
    // First pass — single SHACL alert, dismissed.
    mockUseQualityMetrics.mockReturnValue({
      data: report({
        shacl_pass_rate: {
          value: 0.5,
          trend_delta: 0,
          lower_bound_95: 0,
          upper_bound_95: 1,
        },
      }),
      isLoading: false,
    });
    const { result, rerender } = renderHook(() => useQualityAlerts(), {
      wrapper,
    });
    act(() => {
      result.current.dismiss();
    });
    expect(result.current.visible).toBe(false);

    // Second pass — a different metric starts alerting; the signature
    // changes, so the prior dismissal doesn't suppress the new state.
    mockUseQualityMetrics.mockReturnValue({
      data: report({
        shacl_pass_rate: {
          value: 0.5,
          trend_delta: 0,
          lower_bound_95: 0,
          upper_bound_95: 1,
        },
        query_reproducibility: {
          value: 0.6,
          trend_delta: 0,
          lower_bound_95: 0,
          upper_bound_95: 1,
        },
      }),
      isLoading: false,
    });
    rerender();
    expect(result.current.visible).toBe(true);
  });
});
