// Quality signal → alert derivation.
//
// Pure module: takes a `QualityMetricsReport` and a threshold table,
// returns a list of `QualityAlert` records. No React, no fetching —
// keeps the policy (when is a metric "bad"?) separate from the
// transport (how do we fetch it?) and the presentation (what does
// the banner look like?). Easier to test, easier to tune.

import type { QualityBaseline } from "@/lib/api/quality";
import type { QualityMetricsReport } from "@/types/api";

// ---------------------------------------------------------------------------
// Policy — threshold table
// ---------------------------------------------------------------------------

/** Every forward-facing metric surfaced by `GET /quality/metrics`. */
export type MetricKey =
  | "anchor_match_rate"
  | "glossary_hit_rate"
  | "clarification_success_rate"
  | "query_reproducibility"
  | "shacl_pass_rate"
  | "stale_concept_ratio";

export type AlertSeverity = "warning" | "critical";

export type ThresholdDirection = "higher_is_better" | "lower_is_better";

export interface ThresholdSpec {
  direction: ThresholdDirection;
  /** If the measured value crosses this band, emit a `warning`. */
  warning: number;
  /** If it crosses this band, emit a `critical` (always wins over warning). */
  critical: number;
}

/**
 * Default thresholds — tuned against the Wilson lower-bound display
 * on `/settings/quality?tab=signals`. A metric that spends more than one
 * daily-cron window below the warning band should be banner-worthy;
 * critical is roughly "user-visible quality problem, act today".
 *
 * Keep the table sorted by direction so readers can see at a glance
 * which metrics are reversed.
 */
export const DEFAULT_THRESHOLDS: Record<MetricKey, ThresholdSpec> = {
  // Higher is better — the standard "quality pass" metrics.
  shacl_pass_rate: {
    direction: "higher_is_better",
    warning: 0.9,
    critical: 0.8,
  },
  query_reproducibility: {
    direction: "higher_is_better",
    warning: 0.85,
    critical: 0.7,
  },
  anchor_match_rate: {
    direction: "higher_is_better",
    warning: 0.7,
    critical: 0.5,
  },
  glossary_hit_rate: {
    direction: "higher_is_better",
    warning: 0.5,
    critical: 0.3,
  },
  clarification_success_rate: {
    direction: "higher_is_better",
    warning: 0.75,
    critical: 0.6,
  },
  // Lower is better — stale concepts pile up as ontology drifts.
  stale_concept_ratio: {
    direction: "lower_is_better",
    warning: 0.1,
    critical: 0.2,
  },
};

/**
 * Sample-size floor. Below this, the Wilson 95% CI bands are so wide
 * that a single unlucky execution can flip the banner. Skipping
 * alerts under the floor trades a little latency (we wait for the
 * window to build sample) for much less alert flapping.
 */
export const MIN_SAMPLE_SIZE = 20;

/**
 * Baseline-driven threshold resolution (Phase B).
 *
 * The daily cron computes `median ± k·MAD` per metric per workspace
 * and persists the bundle as `workspace_quality_baseline.thresholds`.
 * When that baseline carries enough signal (`sample_size` at or
 * above `MIN_SAMPLE_SIZE`), each metric's `warn` / `critical` lines
 * are picked from the baseline so alerts reflect this workspace's
 * own distribution instead of the global prior.
 *
 * Falls back to `DEFAULT_THRESHOLDS` when:
 * - `baseline` is `null` (cron hasn't populated the row yet), or
 * - `baseline.sample_size < MIN_SAMPLE_SIZE` (not enough signal to
 *   trust the median / MAD), or
 * - an individual metric is missing from the baseline bundle
 *   (which happens when a new metric is added before the cron's
 *   next run).
 *
 * Per-metric fallback, not all-or-nothing — a new metric inherits
 * the hardcoded prior while established metrics use the adaptive
 * one. Direction is kept from the defaults because the cron's
 * JSONB doesn't carry it (it's a compile-time property).
 */
export function resolveThresholds(
  baseline: QualityBaseline | null | undefined,
  minSampleSize: number = MIN_SAMPLE_SIZE,
): Record<MetricKey, ThresholdSpec> {
  if (!baseline || baseline.sample_size < minSampleSize) {
    return DEFAULT_THRESHOLDS;
  }
  const out: Record<MetricKey, ThresholdSpec> = { ...DEFAULT_THRESHOLDS };
  for (const metric of Object.keys(DEFAULT_THRESHOLDS) as MetricKey[]) {
    const adaptive = baseline.thresholds[metric];
    if (!adaptive) continue;
    out[metric] = {
      direction: DEFAULT_THRESHOLDS[metric].direction,
      warning: adaptive.warn,
      critical: adaptive.critical,
    };
  }
  return out;
}

// ---------------------------------------------------------------------------
// Alert derivation
// ---------------------------------------------------------------------------

export interface QualityAlert {
  metric: MetricKey;
  severity: AlertSeverity;
  /** The measured `.value` from the metric. */
  value: number;
  /** Which threshold got crossed (warning band or critical band). */
  threshold: number;
}

/**
 * Decide which metrics are currently "bad enough" to surface.
 *
 * @param report   The `/quality/metrics` response for the active window.
 * @param thresholds   Optional override — defaults to `DEFAULT_THRESHOLDS`.
 * @param minSampleSize   Optional override — defaults to `MIN_SAMPLE_SIZE`.
 */
export function computeQualityAlerts(
  report: QualityMetricsReport,
  thresholds: Record<MetricKey, ThresholdSpec> = DEFAULT_THRESHOLDS,
  minSampleSize: number = MIN_SAMPLE_SIZE,
): QualityAlert[] {
  // Below the sample-size floor we stay silent — flappy bands make
  // the banner untrustworthy.
  if (report.sample_size < minSampleSize) return [];

  /** @type {QualityAlert[]} */
  const out: QualityAlert[] = [];
  for (const metric of Object.keys(thresholds) as MetricKey[]) {
    const spec = thresholds[metric];
    const value = report[metric].value;

    const isBeyond = (limit: number) =>
      spec.direction === "higher_is_better" ? value < limit : value > limit;

    if (!isBeyond(spec.warning)) continue;
    const severity: AlertSeverity = isBeyond(spec.critical)
      ? "critical"
      : "warning";

    out.push({
      metric,
      severity,
      value,
      threshold: severity === "critical" ? spec.critical : spec.warning,
    });
  }
  return out;
}

/**
 * Stable signature for a set of alerts — used as the sessionStorage
 * dismissal key. Two alert sets collapse to the same signature iff
 * they cover the same metrics at the same severity, regardless of
 * raw value. That way a dismissed banner stays dismissed across
 * small fluctuations but re-appears when a metric crosses into a
 * new severity band or a new metric starts alerting.
 */
export function alertSignature(alerts: QualityAlert[]): string {
  return alerts
    .map((a) => `${a.metric}:${a.severity}`)
    .sort()
    .join(",");
}

/**
 * Pick the most actionable alert to headline in the banner. Critical
 * beats warning; inside a severity bucket, the metrics table's
 * declaration order wins (SHACL first — most user-visible).
 */
export function dominantAlert(alerts: QualityAlert[]): QualityAlert | null {
  if (alerts.length === 0) return null;
  const critical = alerts.filter((a) => a.severity === "critical");
  if (critical.length > 0) return critical[0];
  return alerts[0];
}
