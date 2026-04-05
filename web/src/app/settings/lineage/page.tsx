"use client";

import { useState, useEffect, useCallback } from "react";
import { toast } from "sonner";
import { request } from "@/lib/api/client";
import { Spinner } from "@/components/ui/spinner";
import { SettingsSection } from "@/components/settings/settings-section";
import { cn } from "@/lib/cn";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface LineageSummary {
  graph_label: string;
  graph_element_type: string;
  source_count: number;
  total_records: number;
  last_loaded_at: string | null;
}

interface PropertyMapping {
  source_column: string;
  graph_property: string;
  transform: string | null;
  mapping_kind: string; // "match" | "set"
}

interface LabelMappings {
  label: string;
  element_type: string;
  mappings: PropertyMapping[];
}

interface LineageEntry {
  id: string;
  graph_label: string;
  graph_element_type: string;
  source_type: string;
  source_name: string;
  source_table: string | null;
  source_columns: string[] | null;
  property_mappings: LabelMappings[] | null;
  record_count: number;
  started_at: string;
  completed_at: string | null;
  status: string;
  error_message: string | null;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function formatNumber(n: number) {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return n.toLocaleString();
}

function formatDate(iso: string | null) {
  if (!iso) return "-";
  const d = new Date(iso);
  return d.toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
    year: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function statusBadge(status: string) {
  const map: Record<string, { bg: string; text: string }> = {
    completed: {
      bg: "bg-emerald-50 dark:bg-emerald-950/40",
      text: "text-emerald-700 dark:text-emerald-400",
    },
    running: {
      bg: "bg-blue-50 dark:bg-blue-950/40",
      text: "text-blue-700 dark:text-blue-400",
    },
    failed: {
      bg: "bg-red-50 dark:bg-red-950/40",
      text: "text-red-700 dark:text-red-400",
    },
    partial: {
      bg: "bg-amber-50 dark:bg-amber-950/40",
      text: "text-amber-700 dark:text-amber-400",
    },
  };
  const s = map[status] ?? map.running;
  return (
    <span
      className={cn(
        "inline-flex items-center rounded-full px-2 py-0.5 text-xs font-medium ring-1 ring-inset ring-current/20",
        s.bg,
        s.text,
      )}
    >
      {status}
    </span>
  );
}

// ---------------------------------------------------------------------------
// Aggregation: group all property mappings from entries by (source, label)
// ---------------------------------------------------------------------------

interface MappingGroup {
  sourceTable: string;
  graphLabel: string;
  elementType: string;
  mappings: PropertyMapping[];
}

function aggregateMappings(entries: LineageEntry[]): MappingGroup[] {
  const key = (src: string, label: string) => `${src}||${label}`;
  const groups = new Map<string, MappingGroup>();

  for (const entry of entries) {
    if (!entry.property_mappings) continue;
    const labelMappings = entry.property_mappings;
    for (const lm of labelMappings) {
      const src =
        entry.source_table || entry.source_name || "unknown";
      const k = key(src, lm.label);
      if (!groups.has(k)) {
        groups.set(k, {
          sourceTable: src,
          graphLabel: lm.label,
          elementType: lm.element_type,
          mappings: [],
        });
      }
      const g = groups.get(k)!;
      // Deduplicate by source_column + graph_property
      for (const m of lm.mappings) {
        const exists = g.mappings.some(
          (x) =>
            x.source_column === m.source_column &&
            x.graph_property === m.graph_property,
        );
        if (!exists) {
          g.mappings.push(m);
        }
      }
    }
  }
  return Array.from(groups.values());
}

// ---------------------------------------------------------------------------
// Components
// ---------------------------------------------------------------------------

function MappingCard({
  group,
  expanded,
  onToggle,
}: {
  group: MappingGroup;
  expanded: boolean;
  onToggle: () => void;
}) {
  const matchMappings = group.mappings.filter(
    (m) => m.mapping_kind === "match",
  );
  const setMappings = group.mappings.filter(
    (m) => m.mapping_kind === "set",
  );

  return (
    <div className="rounded-lg border border-zinc-200 dark:border-zinc-700 overflow-hidden">
      <button
        onClick={onToggle}
        className="flex w-full items-center justify-between px-4 py-3 text-left hover:bg-zinc-50 dark:hover:bg-zinc-800/50 transition-colors"
      >
        <div className="flex items-center gap-3">
          {/* Source table */}
          <span className="inline-flex items-center gap-1.5 rounded-md bg-zinc-100 px-2.5 py-1 text-sm font-mono font-medium text-zinc-700 dark:bg-zinc-800 dark:text-zinc-300">
            <svg
              className="h-3.5 w-3.5 text-zinc-400"
              fill="none"
              viewBox="0 0 24 24"
              strokeWidth={1.5}
              stroke="currentColor"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                d="M3.375 19.5h17.25m-17.25 0a1.125 1.125 0 0 1-1.125-1.125M3.375 19.5h7.5c.621 0 1.125-.504 1.125-1.125m-9.75 0V5.625m0 12.75v-1.5c0-.621.504-1.125 1.125-1.125m18.375 2.625V5.625m0 12.75c0 .621-.504 1.125-1.125 1.125m1.125-1.125v-1.5c0-.621-.504-1.125-1.125-1.125m0 3.75h-7.5A1.125 1.125 0 0 1 12 18.375m9.75-12.75c0-.621-.504-1.125-1.125-1.125H3.375c-.621 0-1.125.504-1.125 1.125m19.5 0v1.5c0 .621-.504 1.125-1.125 1.125M2.25 5.625v1.5c0 .621.504 1.125 1.125 1.125m0 0h17.25m-17.25 0h7.5c.621 0 1.125.504 1.125 1.125M3.375 8.25c-.621 0-1.125.504-1.125 1.125v1.5c0 .621.504 1.125 1.125 1.125m17.25-3.75h-7.5c-.621 0-1.125.504-1.125 1.125m8.625-1.125c.621 0 1.125.504 1.125 1.125v1.5c0 .621-.504 1.125-1.125 1.125m-17.25 0h7.5m-7.5 0c-.621 0-1.125.504-1.125 1.125v1.5c0 .621.504 1.125 1.125 1.125M12 10.875v-1.5m0 1.5c0 .621-.504 1.125-1.125 1.125M12 10.875c0 .621.504 1.125 1.125 1.125m-2.25 0c.621 0 1.125.504 1.125 1.125M13.125 12h7.5m-7.5 0c-.621 0-1.125.504-1.125 1.125M20.625 12c.621 0 1.125.504 1.125 1.125v1.5c0 .621-.504 1.125-1.125 1.125m-17.25 0h7.5M12 14.625v-1.5m0 1.5c0 .621-.504 1.125-1.125 1.125M12 14.625c0 .621.504 1.125 1.125 1.125m-2.25 0c.621 0 1.125.504 1.125 1.125m0 0v1.5c0 .621-.504 1.125-1.125 1.125"
              />
            </svg>
            {group.sourceTable}
          </span>

          {/* Arrow */}
          <svg
            className="h-5 w-5 text-zinc-400 flex-shrink-0"
            fill="none"
            viewBox="0 0 24 24"
            strokeWidth={1.5}
            stroke="currentColor"
          >
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              d="M13.5 4.5L21 12m0 0l-7.5 7.5M21 12H3"
            />
          </svg>

          {/* Graph label */}
          <span
            className={cn(
              "inline-flex items-center gap-1.5 rounded-md px-2.5 py-1 text-sm font-mono font-medium",
              group.elementType === "node"
                ? "bg-blue-50 text-blue-700 dark:bg-blue-950/40 dark:text-blue-400"
                : "bg-violet-50 text-violet-700 dark:bg-violet-950/40 dark:text-violet-400",
            )}
          >
            {group.elementType === "node" ? (
              <svg
                className="h-3.5 w-3.5"
                fill="none"
                viewBox="0 0 24 24"
                strokeWidth={1.5}
                stroke="currentColor"
              >
                <circle cx="12" cy="12" r="9" />
              </svg>
            ) : (
              <svg
                className="h-3.5 w-3.5"
                fill="none"
                viewBox="0 0 24 24"
                strokeWidth={1.5}
                stroke="currentColor"
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  d="M7.5 21L3 16.5m0 0L7.5 12M3 16.5h13.5m0-13.5L21 7.5m0 0L16.5 12M21 7.5H7.5"
                />
              </svg>
            )}
            {group.graphLabel}
          </span>

          <span className="text-xs text-zinc-400">
            {group.mappings.length} column{group.mappings.length !== 1 ? "s" : ""}
          </span>
        </div>

        <svg
          className={cn(
            "h-4 w-4 text-zinc-400 transition-transform",
            expanded && "rotate-180",
          )}
          fill="none"
          viewBox="0 0 24 24"
          strokeWidth={2}
          stroke="currentColor"
        >
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            d="M19.5 8.25l-7.5 7.5-7.5-7.5"
          />
        </svg>
      </button>

      {expanded && (
        <div className="border-t border-zinc-100 dark:border-zinc-800 px-4 py-3 bg-zinc-50/50 dark:bg-zinc-900/30">
          {matchMappings.length > 0 && (
            <div className="mb-3">
              <div className="text-[10px] uppercase tracking-wider font-medium text-zinc-400 mb-1.5">
                Identity (match)
              </div>
              <div className="space-y-1">
                {matchMappings.map((m, i) => (
                  <MappingRow key={`match-${i}`} mapping={m} isIdentity />
                ))}
              </div>
            </div>
          )}
          {setMappings.length > 0 && (
            <div>
              <div className="text-[10px] uppercase tracking-wider font-medium text-zinc-400 mb-1.5">
                Properties (set)
              </div>
              <div className="space-y-1">
                {setMappings.map((m, i) => (
                  <MappingRow key={`set-${i}`} mapping={m} />
                ))}
              </div>
            </div>
          )}
          {matchMappings.length === 0 && setMappings.length === 0 && (
            <p className="text-xs text-zinc-400">No column mappings recorded</p>
          )}
        </div>
      )}
    </div>
  );
}

function MappingRow({
  mapping,
  isIdentity,
}: {
  mapping: PropertyMapping;
  isIdentity?: boolean;
}) {
  return (
    <div className="flex items-center gap-2 text-xs">
      <code
        className={cn(
          "rounded px-1.5 py-0.5 font-mono",
          isIdentity
            ? "bg-amber-50 text-amber-700 dark:bg-amber-950/40 dark:text-amber-400"
            : "bg-zinc-100 text-zinc-600 dark:bg-zinc-800 dark:text-zinc-400",
        )}
      >
        {mapping.source_column}
      </code>
      <svg
        className="h-3 w-3 text-zinc-300 dark:text-zinc-600 flex-shrink-0"
        fill="none"
        viewBox="0 0 24 24"
        strokeWidth={2}
        stroke="currentColor"
      >
        <path
          strokeLinecap="round"
          strokeLinejoin="round"
          d="M13.5 4.5L21 12m0 0l-7.5 7.5M21 12H3"
        />
      </svg>
      <code className="rounded bg-blue-50 px-1.5 py-0.5 font-mono text-blue-700 dark:bg-blue-950/40 dark:text-blue-400">
        {mapping.graph_property}
      </code>
      {mapping.transform && (
        <span className="rounded bg-zinc-100 px-1.5 py-0.5 text-[10px] text-zinc-500 dark:bg-zinc-800 dark:text-zinc-500">
          {mapping.transform}
        </span>
      )}
      {isIdentity && (
        <span className="rounded bg-amber-50 px-1 py-0.5 text-[10px] font-medium text-amber-600 dark:bg-amber-950/40 dark:text-amber-400">
          KEY
        </span>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Page
// ---------------------------------------------------------------------------

export default function LineageSettingsPage() {
  const [summary, setSummary] = useState<LineageSummary[]>([]);
  const [entries, setEntries] = useState<LineageEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [expandedCard, setExpandedCard] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      const [summaryData, ...labelResults] = await Promise.all([
        request<LineageSummary[]>("/lineage"),
      ]);
      setSummary(summaryData);

      // Fetch detailed entries for each label to get property mappings
      const labels = summaryData.map((s) => s.graph_label);
      const uniqueLabels = [...new Set(labels)];
      const entryResults = await Promise.all(
        uniqueLabels.map((label) =>
          request<LineageEntry[]>(
            `/lineage/label/${encodeURIComponent(label)}`,
          ),
        ),
      );
      setEntries(entryResults.flat());
    } catch {
      toast.error("Failed to load lineage data");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  const totalRecords = summary.reduce((acc, l) => acc + l.total_records, 0);
  const totalSources = summary.reduce((acc, l) => acc + l.source_count, 0);
  const totalLabels = summary.length;

  // Build mapping groups from entries that have property_mappings
  const mappingGroups = aggregateMappings(entries);

  // Deduplicated entries for history table, sorted by started_at DESC
  const historyEntries = [...entries].sort(
    (a, b) => new Date(b.started_at).getTime() - new Date(a.started_at).getTime(),
  );

  return (
    <div className="space-y-8">
      <SettingsSection
        title="Data Lineage"
        description="Trace how source data flows into your knowledge graph."
      >
        {loading ? (
          <div className="flex items-center justify-center py-12">
            <Spinner />
          </div>
        ) : (
          <>
            {/* Summary cards */}
            <div className="mt-6 grid grid-cols-3 gap-4">
              <div className="rounded-lg border border-zinc-200 p-4 dark:border-zinc-700">
                <div className="text-2xl font-bold text-zinc-900 dark:text-zinc-100">
                  {totalLabels}
                </div>
                <div className="text-xs text-zinc-500">Graph Labels</div>
              </div>
              <div className="rounded-lg border border-zinc-200 p-4 dark:border-zinc-700">
                <div className="text-2xl font-bold text-zinc-900 dark:text-zinc-100">
                  {totalSources}
                </div>
                <div className="text-xs text-zinc-500">Source Tables</div>
              </div>
              <div className="rounded-lg border border-zinc-200 p-4 dark:border-zinc-700">
                <div className="text-2xl font-bold text-zinc-900 dark:text-zinc-100">
                  {formatNumber(totalRecords)}
                </div>
                <div className="text-xs text-zinc-500">Total Records</div>
              </div>
            </div>

            {/* Column-level mappings */}
            {mappingGroups.length > 0 && (
              <div className="mt-8">
                <h2 className="text-sm font-semibold text-zinc-700 dark:text-zinc-300 mb-3">
                  Column Mappings
                </h2>
                <p className="text-xs text-zinc-500 mb-4">
                  Source columns mapped to graph properties during data loading.
                  Click a mapping to expand column-level details.
                </p>
                <div className="space-y-2">
                  {mappingGroups.map((group) => {
                    const cardKey = `${group.sourceTable}||${group.graphLabel}`;
                    return (
                      <MappingCard
                        key={cardKey}
                        group={group}
                        expanded={expandedCard === cardKey}
                        onToggle={() =>
                          setExpandedCard((prev) =>
                            prev === cardKey ? null : cardKey,
                          )
                        }
                      />
                    );
                  })}
                </div>
              </div>
            )}

            {/* Load history table */}
            <div className="mt-8">
              <h2 className="text-sm font-semibold text-zinc-700 dark:text-zinc-300 mb-3">
                Load History
              </h2>
              <div className="overflow-x-auto rounded-lg border border-zinc-200 dark:border-zinc-700">
                <table className="w-full text-sm">
                  <thead>
                    <tr className="border-b border-zinc-200 bg-zinc-50 text-left text-xs font-medium uppercase text-zinc-500 dark:border-zinc-700 dark:bg-zinc-800/50">
                      <th className="py-3 pr-6">Label</th>
                      <th className="py-3 pr-6">Source</th>
                      <th className="py-3 pr-6 text-right">Records</th>
                      <th className="py-3 pr-6 text-right">Started</th>
                      <th className="py-3 pr-6 text-right">Status</th>
                    </tr>
                  </thead>
                  <tbody>
                    {historyEntries.map((e) => (
                      <tr
                        key={e.id}
                        className="border-b border-zinc-100 dark:border-zinc-800 last:border-b-0"
                        title={e.error_message ?? undefined}
                      >
                        <td className="py-3 pr-6 font-medium text-zinc-900 dark:text-zinc-100">
                          <div className="flex items-center gap-1.5">
                            <span
                              className={cn(
                                "inline-block h-2 w-2 rounded-full",
                                e.graph_element_type === "node"
                                  ? "bg-blue-500"
                                  : "bg-violet-500",
                              )}
                            />
                            {e.graph_label}
                          </div>
                        </td>
                        <td className="py-3 pr-6 text-zinc-500">
                          <span className="font-mono text-xs">
                            {e.source_table || e.source_name}
                          </span>
                        </td>
                        <td className="py-3 pr-6 text-right text-zinc-500">
                          {formatNumber(e.record_count)}
                        </td>
                        <td className="py-3 pr-6 text-right text-zinc-500 text-xs">
                          {formatDate(e.started_at)}
                        </td>
                        <td className="py-3 pr-6 text-right">
                          {statusBadge(e.status)}
                        </td>
                      </tr>
                    ))}
                    {historyEntries.length === 0 && (
                      <tr>
                        <td
                          colSpan={5}
                          className="py-8 text-center text-zinc-400"
                        >
                          No load history available
                        </td>
                      </tr>
                    )}
                  </tbody>
                </table>
              </div>
            </div>
          </>
        )}
      </SettingsSection>
    </div>
  );
}
