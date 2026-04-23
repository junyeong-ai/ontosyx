import { describe, it, expect } from "vitest";

import {
  DEFAULT_THRESHOLDS,
  MIN_SAMPLE_SIZE,
  alertSignature,
  computeQualityAlerts,
  dominantAlert,
  type ThresholdSpec,
} from "@/lib/quality/alerts";
import type { QualityMetricsReport } from "@/types/api";

// Tiny builder — every field defaults to a "healthy" value so tests
// only need to override the metric under test.
function report(overrides: Partial<QualityMetricsReport> = {}): QualityMetricsReport {
  const healthy = { value: 0.95, trend_delta: 0, lower_bound_95: 0.9, upper_bound_95: 0.99 };
  const staleHealthy = { value: 0.05, trend_delta: 0, lower_bound_95: 0.03, upper_bound_95: 0.08 };
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

describe("computeQualityAlerts", () => {
  it("returns no alerts when every metric sits above its band", () => {
    expect(computeQualityAlerts(report())).toEqual([]);
  });

  it("stays silent below the sample-size floor even when metrics look bad", () => {
    // A report with only 10 rows has CI bands too wide to trust.
    const bad = report({
      sample_size: 10,
      shacl_pass_rate: { value: 0.4, trend_delta: 0, lower_bound_95: 0.2, upper_bound_95: 0.6 },
    });
    expect(computeQualityAlerts(bad)).toEqual([]);
  });

  it("emits a warning when a higher-is-better metric slips into the warning band", () => {
    const r = report({
      shacl_pass_rate: { value: 0.85, trend_delta: -0.05, lower_bound_95: 0.8, upper_bound_95: 0.9 },
    });
    const alerts = computeQualityAlerts(r);
    expect(alerts).toHaveLength(1);
    expect(alerts[0].metric).toBe("shacl_pass_rate");
    expect(alerts[0].severity).toBe("warning");
    expect(alerts[0].threshold).toBe(DEFAULT_THRESHOLDS.shacl_pass_rate.warning);
  });

  it("emits a critical when the metric drops below the critical band", () => {
    const r = report({
      shacl_pass_rate: { value: 0.5, trend_delta: -0.3, lower_bound_95: 0.4, upper_bound_95: 0.6 },
    });
    const [alert] = computeQualityAlerts(r);
    expect(alert.severity).toBe("critical");
    expect(alert.threshold).toBe(DEFAULT_THRESHOLDS.shacl_pass_rate.critical);
  });

  it("treats stale_concept_ratio as lower-is-better", () => {
    // Stale ratio is the one reversed metric — 0.25 is BAD, not good.
    const r = report({
      stale_concept_ratio: { value: 0.25, trend_delta: 0.1, lower_bound_95: 0.2, upper_bound_95: 0.3 },
    });
    const [alert] = computeQualityAlerts(r);
    expect(alert.metric).toBe("stale_concept_ratio");
    expect(alert.severity).toBe("critical");
  });

  it("emits multiple alerts when multiple metrics cross bands", () => {
    const r = report({
      shacl_pass_rate: { value: 0.5, trend_delta: 0, lower_bound_95: 0, upper_bound_95: 1 },
      query_reproducibility: { value: 0.8, trend_delta: 0, lower_bound_95: 0, upper_bound_95: 1 },
    });
    const alerts = computeQualityAlerts(r);
    const metrics = alerts.map((a) => a.metric).sort();
    expect(metrics).toEqual(["query_reproducibility", "shacl_pass_rate"]);
    // Pass rate at 0.5 is below critical (0.8) → critical.
    // Reproducibility at 0.8 is below warning (0.85) but above
    // critical (0.7) → warning.
    const byMetric = Object.fromEntries(alerts.map((a) => [a.metric, a.severity]));
    expect(byMetric.shacl_pass_rate).toBe("critical");
    expect(byMetric.query_reproducibility).toBe("warning");
  });

  it("honours a custom threshold table", () => {
    const thresholds: Record<"shacl_pass_rate", ThresholdSpec> = {
      shacl_pass_rate: {
        direction: "higher_is_better",
        warning: 0.99, // unrealistically strict
        critical: 0.95,
      },
    };
    const r = report({
      shacl_pass_rate: { value: 0.97, trend_delta: 0, lower_bound_95: 0, upper_bound_95: 1 },
    });
    // With the default thresholds 0.97 sits above warning (0.9) and
    // emits nothing. With the stricter override 0.97 is inside the
    // warning band (below 0.99 but above 0.95).
    expect(computeQualityAlerts(r)).toEqual([]);
    const custom = computeQualityAlerts(r, thresholds as never, MIN_SAMPLE_SIZE);
    expect(custom).toHaveLength(1);
    expect(custom[0].severity).toBe("warning");
  });
});

describe("alertSignature", () => {
  it("returns the same string for the same metric-severity pairs", () => {
    const a1 = [
      { metric: "shacl_pass_rate", severity: "critical", value: 0.5, threshold: 0.8 },
      { metric: "query_reproducibility", severity: "warning", value: 0.8, threshold: 0.85 },
    ] as const;
    // Reverse order — signature is stable.
    const a2 = [a1[1], a1[0]] as const;
    expect(alertSignature([...a1])).toBe(alertSignature([...a2]));
  });

  it("changes when a metric flips severity", () => {
    const a1 = [{ metric: "shacl_pass_rate", severity: "warning", value: 0.85, threshold: 0.9 }] as const;
    const a2 = [{ metric: "shacl_pass_rate", severity: "critical", value: 0.5, threshold: 0.8 }] as const;
    expect(alertSignature([...a1])).not.toBe(alertSignature([...a2]));
  });
});

describe("dominantAlert", () => {
  it("returns null for an empty list", () => {
    expect(dominantAlert([])).toBeNull();
  });

  it("prefers critical over warning regardless of order", () => {
    const warn = { metric: "glossary_hit_rate", severity: "warning", value: 0.4, threshold: 0.5 } as const;
    const crit = { metric: "shacl_pass_rate", severity: "critical", value: 0.5, threshold: 0.8 } as const;
    expect(dominantAlert([warn, crit])?.metric).toBe("shacl_pass_rate");
    expect(dominantAlert([crit, warn])?.metric).toBe("shacl_pass_rate");
  });
});
