"use client";

import { useTranslations } from "next-intl";
import { Spinner } from "@/components/ui/spinner";
import type { SourceHistoryEntry } from "@/types/api";

// ---------------------------------------------------------------------------
// Progress indicator for streaming design/refine operations
// ---------------------------------------------------------------------------

/** Known phase wire values — unknown variants fall through to the raw string. */
const KNOWN_PHASES = [
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
    <div className="flex items-center gap-2 rounded-lg border border-emerald-200 bg-emerald-50/50 px-3 py-2 dark:border-emerald-900 dark:bg-emerald-950/20">
      <Spinner size="xs" className="shrink-0 text-emerald-500" />
      <div className="min-w-0 flex-1">
        <p className="text-xs font-medium text-emerald-700 dark:text-emerald-300">
          {label}
        </p>
        {detail && (
          <p className="truncate text-[10px] text-emerald-600/70 dark:text-emerald-400/70">
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
      <summary className="cursor-pointer text-[10px] font-semibold uppercase tracking-wider text-muted-foreground hover:text-zinc-600 dark:hover:text-zinc-300">
        {t("sourcesTitle")}
        <span className="ml-1.5 text-[10px] font-normal normal-case">
          {t("sourceCount", { count: entries.length })}
        </span>
      </summary>
      <div className="mt-1.5 space-y-1">
        {entries.map((entry, i) => (
          <div
            key={`${entry.source_type}-${entry.added_at}-${i}`}
            className="rounded border border-zinc-100 px-2 py-1.5 dark:border-zinc-800"
          >
            <div className="flex items-center gap-2 text-[10px] text-zinc-600 dark:text-muted-foreground">
              <span className="inline-flex shrink-0 rounded bg-zinc-100 px-1.5 py-0.5 font-medium dark:bg-zinc-800">
                {isKnownSourceType(entry.source_type)
                  ? t(`sourceTypes.${entry.source_type}`)
                  : entry.source_type}
              </span>
              <span className="min-w-0 truncate font-medium">
                {entry.schema_name ?? entry.url ?? t("inlineSource")}
              </span>
              <span className="ml-auto shrink-0 text-muted-foreground">
                {new Date(entry.added_at).toLocaleDateString()}
              </span>
            </div>
            {entry.fingerprint && (
              <p className="mt-0.5 truncate pl-0.5 text-[9px] font-mono text-muted-foreground dark:text-zinc-600">
                {entry.fingerprint}
              </p>
            )}
          </div>
        ))}
      </div>
    </details>
  );
}
