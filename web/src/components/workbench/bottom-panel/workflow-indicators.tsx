"use client";

import { useTranslations } from "next-intl";
import { Spinner } from "@/components/ui/spinner";
import type { SourceHistoryEntry } from "@/types/api";

// ---------------------------------------------------------------------------
// Progress indicator for streaming design/refine operations
// ---------------------------------------------------------------------------

/** Known phase wire values — unknown variants fall through to the raw string. */
const KNOWN_PHASES = [
  "starting",
  "validating",
  "clustering",
  "designing",
  "merging",
  "resolving_edges",
  "profiling",
  "profiling_complete",
  "refining",
  "reconciling",
  "assessing_quality",
  "persisting",
] as const;
type KnownPhase = (typeof KNOWN_PHASES)[number];
function isKnownPhase(s: string): s is KnownPhase {
  return (KNOWN_PHASES as readonly string[]).includes(s);
}

export function ProgressIndicator({
  phase,
  detail,
}: {
  phase: string;
  detail: string | null;
}) {
  const t = useTranslations("workbench.bottomPanel.workflowIndicators.phases");
  const label = isKnownPhase(phase) ? t(phase) : phase;

  return (
    <div className="flex items-center gap-2 rounded-lg border border-brand-border bg-brand-surface px-3 py-2">
      <Spinner size="xs" className="shrink-0 text-brand-foreground" />
      <div className="min-w-0 flex-1">
        <p className="text-xs font-medium text-brand-foreground-strong">
          {label}
        </p>
        {detail && (
          <p className="truncate text-xs text-brand-foreground/70">
            {detail}
          </p>
        )}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Source history display
// ---------------------------------------------------------------------------

const KNOWN_SOURCE_TYPES = [
  "text",
  "csv",
  "json",
  "postgresql",
  "code_repository",
  "ontology",
] as const;
type KnownSourceType = (typeof KNOWN_SOURCE_TYPES)[number];
function isKnownSourceType(s: string): s is KnownSourceType {
  return (KNOWN_SOURCE_TYPES as readonly string[]).includes(s);
}

export function SourceHistorySection({ entries }: { entries: SourceHistoryEntry[] }) {
  const t = useTranslations("workbench.bottomPanel.workflowIndicators");
  const hasMultiple = entries.length > 1;
  return (
    <details className="text-xs" open={hasMultiple}>
      <summary className="cursor-pointer text-xs font-semibold uppercase tracking-wider text-foreground-muted hover:text-foreground-muted">
        {t("sourcesTitle")}
        <span className="ms-1.5 text-xs font-normal normal-case">
          {t("sourceCount", { count: entries.length })}
        </span>
      </summary>
      <div className="mt-1.5 space-y-1">
        {entries.map((entry, i) => (
          <div
            key={`${entry.source_type}-${entry.added_at}-${i}`}
            className="rounded border border-divider-soft px-2 py-1.5"
          >
            <div className="flex items-center gap-2 text-xs text-foreground">
              <span className="inline-flex shrink-0 rounded bg-surface-inset px-1.5 py-0.5 font-medium">
                {isKnownSourceType(entry.source_type)
                  ? t(`sourceTypes.${entry.source_type}`)
                  : entry.source_type}
              </span>
              <span className="min-w-0 truncate font-medium">
                {entry.schema_name ?? entry.url ?? t("inlineSource")}
              </span>
              <span className="ms-auto shrink-0 text-foreground-muted">
                {new Date(entry.added_at).toLocaleDateString()}
              </span>
            </div>
            {entry.fingerprint && (
              <p className="mt-0.5 truncate ps-0.5 text-2xs font-mono text-foreground-muted">
                {entry.fingerprint}
              </p>
            )}
          </div>
        ))}
      </div>
    </details>
  );
}
