/**
 * Localize a `QualityGap` into its issue + suggestion strings using a
 * translator scoped to `qualityGap`. The backend never produces user-facing
 * prose; the FE i18n catalogue owns the locale.
 *
 * Caller is expected to pass `useTranslations("qualityGap")`. Key resolution:
 * - Most categories: `<category>.{issue,suggestion}`
 * - `missing_description`: `missing_description.<location.ref_type>.{issue,suggestion}`
 *   — the same category fires for nodes, node-properties, edges, and edge-properties,
 *   so the location's ref_type picks the variant.
 *
 * `gap.params` is passed straight through to the translator; each i18n
 * value can interpolate the structured fields the backend emitted.
 */

import type { QualityGap } from "@/types/api";

type ScopedTranslator = (key: string, values?: Record<string, string | number>) => string;

function keyFor(gap: QualityGap, suffix: "issue" | "suggestion"): string {
  if (gap.category === "missing_description") {
    return `missing_description.${gap.location.ref_type}.${suffix}`;
  }
  return `${gap.category}.${suffix}`;
}

export function localizeQualityGapIssue(gap: QualityGap, t: ScopedTranslator): string {
  return t(keyFor(gap, "issue"), gap.params);
}

export function localizeQualityGapSuggestion(gap: QualityGap, t: ScopedTranslator): string {
  return t(keyFor(gap, "suggestion"), gap.params);
}

export function localizeQualityGap(
  gap: QualityGap,
  t: ScopedTranslator,
): { issue: string; suggestion: string } {
  return {
    issue: localizeQualityGapIssue(gap, t),
    suggestion: localizeQualityGapSuggestion(gap, t),
  };
}
