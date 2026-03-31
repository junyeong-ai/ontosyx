"use client";

import { Spinner } from "@/components/ui/spinner";
import type { SourceHistoryEntry } from "@/types/api";

// ---------------------------------------------------------------------------
// Progress indicator for streaming design/refine operations
// ---------------------------------------------------------------------------

export const PHASE_LABELS: Record<string, string> = {
  validating: "Validating input...",
  clustering: "Analyzing table relationships...",
  designing: "Designing ontology...",
  merging: "Merging partial ontologies...",
  resolving_edges: "Resolving cross-domain edges...",
  profiling: "Profiling graph data...",
  profiling_complete: "Profiling complete",
  refining: "Refining ontology...",
  reconciling: "Reconciling changes...",
  assessing_quality: "Assessing quality...",
  persisting: "Saving results...",
};

export function ProgressIndicator({
  phase,
  detail,
}: {
  phase: string;
  detail: string | null;
}) {
  const label = PHASE_LABELS[phase] ?? phase;

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

export const SOURCE_TYPE_LABELS: Record<string, string> = {
  text: "Text",
  csv: "CSV",
  json: "JSON",
  postgresql: "PostgreSQL",
  code_repository: "Code Repo",
  ontology: "Ontology",
};

export function SourceHistorySection({ entries }: { entries: SourceHistoryEntry[] }) {
  const hasMultiple = entries.length > 1;
  return (
    <details className="text-xs" open={hasMultiple}>
      <summary className="cursor-pointer text-[10px] font-semibold uppercase tracking-wider text-zinc-400 hover:text-zinc-600 dark:hover:text-zinc-300">
        Sources
        <span className="ml-1.5 text-[10px] font-normal normal-case">
          {entries.length} {entries.length === 1 ? "source" : "sources"}
        </span>
      </summary>
      <div className="mt-1.5 space-y-1">
        {entries.map((entry, i) => (
          <div
            key={`${entry.source_type}-${entry.added_at}-${i}`}
            className="rounded border border-zinc-100 px-2 py-1.5 dark:border-zinc-800"
          >
            <div className="flex items-center gap-2 text-[10px] text-zinc-600 dark:text-zinc-400">
              <span className="inline-flex shrink-0 rounded bg-zinc-100 px-1.5 py-0.5 font-medium dark:bg-zinc-800">
                {SOURCE_TYPE_LABELS[entry.source_type] ?? entry.source_type}
              </span>
              <span className="min-w-0 truncate font-medium">
                {entry.schema_name ?? entry.url ?? "inline"}
              </span>
              <span className="ml-auto shrink-0 text-zinc-400 dark:text-zinc-500">
                {new Date(entry.added_at).toLocaleDateString()}
              </span>
            </div>
            {entry.fingerprint && (
              <p className="mt-0.5 truncate pl-0.5 text-[9px] font-mono text-zinc-400 dark:text-zinc-600">
                {entry.fingerprint}
              </p>
            )}
          </div>
        ))}
      </div>
    </details>
  );
}
