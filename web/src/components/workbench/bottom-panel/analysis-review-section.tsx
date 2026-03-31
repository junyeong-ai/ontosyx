"use client";

import { useCallback, useMemo, useState } from "react";
import { HugeiconsIcon } from "@hugeicons/react";
import { Alert01Icon, MagicWand01Icon } from "@hugeicons/core-free-icons";
import { FormInput } from "@/components/ui/form-input";
import { cn } from "@/lib/cn";
import { toast } from "sonner";
import type {
  PiiDecision,
  PiiFinding,
  AmbiguousColumn,
  ImpliedRelationship,
  TableExclusionSuggestion,
  SourceAnalysisReport,
} from "@/types/api";
import { relationshipKey, columnKey, selectClassName } from "./design-panel-shared";

// ---------------------------------------------------------------------------
// PII auto-fill — defaults to "allow" for internal tools.
// Users review and override individual decisions as needed.
// ---------------------------------------------------------------------------

function inferPiiDecision(_finding: PiiFinding): PiiDecision {
  // Internal backoffice tool: default to "allow" for maximum data availability.
  // Users review and change individual decisions to "mask"/"exclude" as needed.
  return "allow";
}

// ---------------------------------------------------------------------------
// Column clarification auto-fill heuristics
// ---------------------------------------------------------------------------

function inferClarification(column: AmbiguousColumn): string {
  const col = column.column.toLowerCase();
  const samples = column.sample_values;

  // Year: 4-digit numbers
  if (/year/.test(col) && samples.every((v) => /^\d{4}$/.test(v.trim()))) {
    return "Calendar year (e.g., YYYY)";
  }

  // Age: 2-digit numbers
  if (/age/.test(col) && samples.every((v) => /^\d{1,3}$/.test(v.trim()))) {
    return "Age in years";
  }

  // Percentage
  if (/pct|percent/.test(col)) {
    return "Percentage value (0-100)";
  }

  // Rating
  if (/rating/.test(col) && samples.every((v) => /^\d{1,2}$/.test(v.trim()))) {
    const nums = samples.map((v) => Number(v.trim())).filter((n) => !isNaN(n));
    if (nums.length > 0) {
      return `Rating scale (${Math.min(...nums)}-${Math.max(...nums)})`;
    }
    return "Rating scale";
  }

  // Grade
  if (/grade/.test(col)) {
    return "Grade or level classification";
  }

  // Quantity
  if (/quantity|qty/.test(col)) {
    return "Quantity/count of items";
  }

  // Type/status — list sample values as categories
  if (/type|status|category|kind/.test(col) && samples.length > 0) {
    return `Category: ${samples.join(", ")}`;
  }

  // Default: readable column name + sample context
  const readable = col
    .replace(/_/g, " ")
    .replace(/\b\w/g, (c) => c.toUpperCase());
  if (samples.length > 0) {
    return `${readable} (values: ${samples.slice(0, 5).join(", ")})`;
  }
  return readable;
}

// ---------------------------------------------------------------------------
// Grouping helpers — group items by source table name
// ---------------------------------------------------------------------------

function groupByTable<T>(items: T[], getTable: (item: T) => string): Map<string, T[]> {
  const map = new Map<string, T[]>();
  for (const item of items) {
    const table = getTable(item);
    const group = map.get(table);
    if (group) {
      group.push(item);
    } else {
      map.set(table, [item]);
    }
  }
  return map;
}

// ---------------------------------------------------------------------------
// Grouped section with <details> collapse per table
// ---------------------------------------------------------------------------

function GroupedSection({
  title,
  groups,
  searchFilter,
  unresolvedOnly,
  getUnresolvedCount,
  renderItem,
  renderBatchAction,
}: {
  title: string;
  groups: Map<string, { key: string; item: unknown }[]>;
  searchFilter: string;
  unresolvedOnly: boolean;
  getUnresolvedCount: (tableName: string) => number;
  renderItem: (entry: { key: string; item: unknown }) => React.ReactNode;
  renderBatchAction?: (tableName: string) => React.ReactNode;
}) {
  if (groups.size === 0) return null;

  const lowerSearch = searchFilter.toLowerCase();

  // Filter groups by search and unresolved
  const filteredGroups = Array.from(groups.entries())
    .filter(([tableName]) => !lowerSearch || tableName.toLowerCase().includes(lowerSearch))
    .filter(([tableName]) => !unresolvedOnly || getUnresolvedCount(tableName) > 0)
    .sort(([a], [b]) => a.localeCompare(b));

  if (filteredGroups.length === 0) return null;

  return (
    <div>
      <h4 className="mb-1 text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
        {title}
      </h4>
      <div className="space-y-1">
        {filteredGroups.map(([tableName, entries]) => {
          const unresolved = getUnresolvedCount(tableName);
          return (
            <details key={tableName} open={unresolved > 0}>
              <summary className="flex cursor-pointer select-none items-center gap-2 rounded border border-zinc-200 bg-zinc-100/60 px-2 py-1 text-[10px] font-medium text-zinc-700 hover:bg-zinc-100 dark:border-zinc-800 dark:bg-zinc-900/60 dark:text-zinc-300 dark:hover:bg-zinc-800/60">
                <span className="flex-1">{tableName}</span>
                {renderBatchAction?.(tableName)}
                {unresolved > 0 && (
                  <span className="rounded-full bg-amber-100 px-1.5 py-0.5 text-[9px] font-medium text-amber-800 dark:bg-amber-900/40 dark:text-amber-300">
                    {unresolved}
                  </span>
                )}
              </summary>
              <div className="mt-1 space-y-1 pl-2">
                {entries.map((entry) => (
                  <div key={entry.key}>{renderItem(entry)}</div>
                ))}
              </div>
            </details>
          );
        })}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Analysis Review Section — decision UI for analysis report findings
// ---------------------------------------------------------------------------

export function AnalysisReviewSection({
  report,
  confirmedRelationships,
  setConfirmedRelationships,
  piiDecisions,
  setPiiDecisions,
  clarifications,
  setClarifications,
  excludedTables,
  setExcludedTables,
  allowPartialAnalysis,
  setAllowPartialAnalysis,
  unresolvedPiiCount,
  unresolvedClarificationCount,
  needsPartialAcknowledgement,
}: {
  report: SourceAnalysisReport;
  confirmedRelationships: Record<string, boolean>;
  setConfirmedRelationships: React.Dispatch<
    React.SetStateAction<Record<string, boolean>>
  >;
  piiDecisions: Record<string, PiiDecision | "">;
  setPiiDecisions: React.Dispatch<
    React.SetStateAction<Record<string, PiiDecision | "">>
  >;
  clarifications: Record<string, string>;
  setClarifications: React.Dispatch<
    React.SetStateAction<Record<string, string>>
  >;
  excludedTables: Record<string, boolean>;
  setExcludedTables: React.Dispatch<
    React.SetStateAction<Record<string, boolean>>
  >;
  allowPartialAnalysis: boolean;
  setAllowPartialAnalysis: (v: boolean) => void;
  unresolvedPiiCount: number;
  unresolvedClarificationCount: number;
  needsPartialAcknowledgement: boolean;
}) {
  const [searchFilter, setSearchFilter] = useState("");
  const [unresolvedOnly, setUnresolvedOnly] = useState(true);

  // -----------------------------------------------------------------------
  // Resolution counts
  // -----------------------------------------------------------------------

  const totalItems = useMemo(() => {
    return (
      report.implied_relationships.length +
      report.pii_findings.length +
      report.ambiguous_columns.length +
      report.table_exclusion_suggestions.length
    );
  }, [report]);

  const unresolvedRelCount = useMemo(() => {
    return report.implied_relationships.filter((rel) => {
      const key = relationshipKey(rel);
      return !confirmedRelationships[key];
    }).length;
  }, [report.implied_relationships, confirmedRelationships]);

  const unresolvedExcludedCount = useMemo(() => {
    return report.table_exclusion_suggestions.filter(
      (s) => !excludedTables[s.table_name],
    ).length;
  }, [report.table_exclusion_suggestions, excludedTables]);

  const totalUnresolved = unresolvedRelCount + unresolvedPiiCount + unresolvedClarificationCount + unresolvedExcludedCount;
  const totalResolved = totalItems - totalUnresolved;
  const progressPercent = totalItems > 0 ? Math.round((totalResolved / totalItems) * 100) : 100;

  // -----------------------------------------------------------------------
  // Grouped data
  // -----------------------------------------------------------------------

  const relGroups = useMemo(() => {
    const grouped = groupByTable(
      report.implied_relationships,
      (rel) => rel.from_table,
    );
    const result = new Map<string, { key: string; item: ImpliedRelationship }[]>();
    for (const [table, items] of grouped) {
      result.set(
        table,
        items.map((rel) => ({ key: relationshipKey(rel), item: rel })),
      );
    }
    return result;
  }, [report.implied_relationships]);

  const piiGroups = useMemo(() => {
    const grouped = groupByTable(report.pii_findings, (f) => f.table);
    const result = new Map<string, { key: string; item: PiiFinding }[]>();
    for (const [table, items] of grouped) {
      result.set(
        table,
        items.map((f) => ({ key: columnKey(f.table, f.column), item: f })),
      );
    }
    return result;
  }, [report.pii_findings]);

  const clarGroups = useMemo(() => {
    const grouped = groupByTable(report.ambiguous_columns, (c) => c.table);
    const result = new Map<string, { key: string; item: AmbiguousColumn }[]>();
    for (const [table, items] of grouped) {
      result.set(
        table,
        items.map((c) => ({ key: columnKey(c.table, c.column), item: c })),
      );
    }
    return result;
  }, [report.ambiguous_columns]);

  const excludedGroups = useMemo(() => {
    const result = new Map<string, { key: string; item: TableExclusionSuggestion }[]>();
    for (const s of report.table_exclusion_suggestions) {
      result.set(s.table_name, [{ key: s.table_name, item: s }]);
    }
    return result;
  }, [report.table_exclusion_suggestions]);

  // -----------------------------------------------------------------------
  // Per-table unresolved counts
  // -----------------------------------------------------------------------

  const relUnresolvedByTable = useCallback(
    (tableName: string) => {
      const items = relGroups.get(tableName);
      if (!items) return 0;
      return items.filter((e) => !confirmedRelationships[e.key]).length;
    },
    [relGroups, confirmedRelationships],
  );

  const piiUnresolvedByTable = useCallback(
    (tableName: string) => {
      const items = piiGroups.get(tableName);
      if (!items) return 0;
      return items.filter((e) => !piiDecisions[e.key]).length;
    },
    [piiGroups, piiDecisions],
  );

  const clarUnresolvedByTable = useCallback(
    (tableName: string) => {
      const items = clarGroups.get(tableName);
      if (!items) return 0;
      return items.filter((e) => !clarifications[e.key]?.trim()).length;
    },
    [clarGroups, clarifications],
  );

  const excludedUnresolvedByTable = useCallback(
    (tableName: string) => {
      return excludedTables[tableName] ? 0 : 1;
    },
    [excludedTables],
  );

  // -----------------------------------------------------------------------
  // Batch accept per-table
  // -----------------------------------------------------------------------

  const acceptAllRelInTable = useCallback(
    (tableName: string) => {
      const items = relGroups.get(tableName);
      if (!items) return;
      const updates: Record<string, boolean> = {};
      for (const e of items) updates[e.key] = true;
      setConfirmedRelationships((prev) => ({ ...prev, ...updates }));
    },
    [relGroups, setConfirmedRelationships],
  );

  const acceptAllPiiInTable = useCallback(
    (tableName: string) => {
      const items = piiGroups.get(tableName);
      if (!items) return;
      const updates: Record<string, PiiDecision | ""> = {};
      for (const e of items) {
        if (!piiDecisions[e.key]) {
          updates[e.key] = inferPiiDecision(e.item as PiiFinding);
        }
      }
      if (Object.keys(updates).length > 0) {
        setPiiDecisions((prev) => ({ ...prev, ...updates }));
      }
    },
    [piiGroups, piiDecisions, setPiiDecisions],
  );

  const acceptAllClarInTable = useCallback(
    (tableName: string) => {
      const items = clarGroups.get(tableName);
      if (!items) return;
      const updates: Record<string, string> = {};
      for (const e of items) {
        if (!clarifications[e.key]?.trim()) {
          const col = e.item as AmbiguousColumn;
          updates[e.key] = col.repo_suggestion
            ? col.repo_suggestion.suggested_values
            : inferClarification(col);
        }
      }
      if (Object.keys(updates).length > 0) {
        setClarifications((prev) => ({ ...prev, ...updates }));
      }
    },
    [clarGroups, clarifications, setClarifications],
  );

  const acceptAllExcludedInTable = useCallback(
    (tableName: string) => {
      setExcludedTables((prev) => ({ ...prev, [tableName]: true }));
    },
    [setExcludedTables],
  );

  // -----------------------------------------------------------------------
  // Global auto-fill
  // -----------------------------------------------------------------------

  const autoFill = useCallback(() => {
    let piiCount = 0;
    let clarCount = 0;

    // Auto-fill unresolved PII decisions
    if (report.pii_findings.length > 0) {
      const newPii: Record<string, PiiDecision | ""> = {};
      for (const finding of report.pii_findings) {
        const key = columnKey(finding.table, finding.column);
        if (!piiDecisions[key]) {
          newPii[key] = inferPiiDecision(finding);
          piiCount++;
        }
      }
      if (piiCount > 0) {
        setPiiDecisions((prev) => ({ ...prev, ...newPii }));
      }
    }

    // Auto-fill unresolved clarifications
    if (report.ambiguous_columns.length > 0) {
      const newClar: Record<string, string> = {};
      for (const column of report.ambiguous_columns) {
        const key = columnKey(column.table, column.column);
        if (!clarifications[key]?.trim()) {
          if (column.repo_suggestion) {
            newClar[key] = column.repo_suggestion.suggested_values;
          } else {
            newClar[key] = inferClarification(column);
          }
          clarCount++;
        }
      }
      if (clarCount > 0) {
        setClarifications((prev) => ({ ...prev, ...newClar }));
      }
    }

    if (piiCount === 0 && clarCount === 0) {
      toast.info("All decisions are already filled");
    } else {
      toast.success(
        `Filled ${piiCount} PII decision${piiCount !== 1 ? "s" : ""} and ${clarCount} clarification${clarCount !== 1 ? "s" : ""} with defaults`,
        { description: "Review each decision before designing — adjust as needed" },
      );
    }
  }, [report, piiDecisions, setPiiDecisions, clarifications, setClarifications]);

  const hasUnresolved = unresolvedPiiCount > 0 || unresolvedClarificationCount > 0;

  return (
    <div className="space-y-3 rounded-lg border border-zinc-200 bg-zinc-50/70 p-3 dark:border-zinc-800 dark:bg-zinc-900/50">
      {/* Summary */}
      <div className="flex items-center justify-between gap-3">
        <div>
          <p className="text-xs font-semibold text-zinc-700 dark:text-zinc-200">
            Analysis Review
          </p>
          <p className="mt-0.5 text-[10px] text-zinc-500">
            {report.schema_stats.table_count} tables, {report.schema_stats.column_count} columns,{" "}
            {report.schema_stats.declared_fk_count} FKs · Remaining: {unresolvedPiiCount} PII,{" "}
            {unresolvedClarificationCount} clarifications
            {needsPartialAcknowledgement ? ", partial ack" : ""}
          </p>
        </div>
        <div className="flex items-center gap-2">
          {hasUnresolved && (
            <button
              type="button"
              onClick={autoFill}
              className={cn(
                "flex items-center gap-1 rounded-md border px-2 py-1 text-[10px] font-medium transition-colors",
                "border-violet-300 bg-violet-50 text-violet-700 hover:bg-violet-100",
                "dark:border-violet-700 dark:bg-violet-950 dark:text-violet-300 dark:hover:bg-violet-900",
              )}
            >
              <HugeiconsIcon icon={MagicWand01Icon} className="h-3 w-3" size="100%" />
              Auto-fill
            </button>
          )}
          <span
            className={cn(
              "rounded-full px-1.5 py-0.5 text-[9px] font-medium uppercase",
              report.analysis_completeness === "partial"
                ? "bg-amber-100 text-amber-800"
                : "bg-emerald-100 text-emerald-800",
            )}
          >
            {report.analysis_completeness}
          </span>
        </div>
      </div>

      {/* Progress bar */}
      {totalItems > 0 && (
        <div>
          <div className="mb-1 flex items-center justify-between text-[10px] text-zinc-500">
            <span>{progressPercent}% resolved ({totalResolved}/{totalItems})</span>
            <span className="text-zinc-400">{totalUnresolved} remaining</span>
          </div>
          <div className="h-1.5 w-full overflow-hidden rounded-full bg-zinc-200 dark:bg-zinc-800">
            <div
              className={cn(
                "h-full rounded-full transition-all duration-300",
                progressPercent === 100
                  ? "bg-emerald-500"
                  : progressPercent >= 50
                    ? "bg-emerald-400"
                    : "bg-amber-400",
              )}
              style={{ width: `${progressPercent}%` }}
            />
          </div>
        </div>
      )}

      {/* Filter bar */}
      {totalItems > 0 && (
        <div className="flex flex-wrap items-center gap-2 rounded-md border border-zinc-200 bg-white px-2 py-1.5 dark:border-zinc-800 dark:bg-zinc-950/60">
          <label className="flex items-center gap-1.5 text-[10px] text-zinc-600 dark:text-zinc-300">
            <input
              type="checkbox"
              checked={unresolvedOnly}
              onChange={(e) => setUnresolvedOnly(e.target.checked)}
              className="accent-emerald-600"
            />
            Unresolved only
          </label>
          <div className="h-3 w-px bg-zinc-200 dark:bg-zinc-700" />
          <input
            type="text"
            value={searchFilter}
            onChange={(e) => setSearchFilter(e.target.value)}
            placeholder="Filter by table name..."
            className="flex-1 border-none bg-transparent text-[10px] text-zinc-700 outline-none placeholder:text-zinc-500 dark:text-zinc-200 dark:placeholder:text-zinc-500"
          />
          <span className="whitespace-nowrap text-[10px] font-medium text-zinc-500">
            {totalUnresolved}/{totalItems} unresolved
          </span>
        </div>
      )}

      {/* Warnings */}
      {report.analysis_warnings.length > 0 && (
        <div className="rounded-md border border-amber-200 bg-amber-50 p-2 dark:border-amber-900 dark:bg-amber-950/40">
          <div className="flex items-center gap-1.5">
            <HugeiconsIcon icon={Alert01Icon} className="h-3 w-3 text-amber-600" size="100%" />
            <span className="text-xs font-medium text-amber-900 dark:text-amber-100">
              Partial Analysis Warnings
            </span>
          </div>
          <div className="mt-2 space-y-1">
            {report.analysis_warnings.map((w) => (
              <p key={`${w.kind}-${w.location}`} className="text-[10px] text-zinc-600 dark:text-zinc-300">
                <span className="font-medium">{w.location}</span>: {w.message}
              </p>
            ))}
          </div>
          <label className="mt-2 flex items-start gap-1.5 text-[10px] text-zinc-600 dark:text-zinc-300">
            <input
              type="checkbox"
              checked={allowPartialAnalysis}
              onChange={(e) => setAllowPartialAnalysis(e.target.checked)}
              className="mt-0.5"
            />
            I understand the analysis is partial.
          </label>
        </div>
      )}

      {/* Repo summary */}
      {report.repo_summary && (
        <div className="text-[10px] text-zinc-500">
          Repo: {report.repo_summary.status} · {report.repo_summary.files_analyzed}/{report.repo_summary.files_requested} files
          {report.repo_summary.enums_found > 0 && ` · ${report.repo_summary.enums_found} enums`}
        </div>
      )}

      {/* Relationships — grouped by from_table */}
      {report.implied_relationships.length > 0 && (
        <GroupedSection
          title="Confirm Relationships"
          groups={relGroups as Map<string, { key: string; item: unknown }[]>}
          searchFilter={searchFilter}
          unresolvedOnly={unresolvedOnly}
          getUnresolvedCount={relUnresolvedByTable}
          renderBatchAction={(tableName) => {
            const unresolved = relUnresolvedByTable(tableName);
            return unresolved > 0 ? (
              <button
                type="button"
                onClick={(e) => {
                  e.preventDefault();
                  acceptAllRelInTable(tableName);
                }}
                className="rounded bg-emerald-100 px-1.5 py-0.5 text-[9px] font-medium text-emerald-700 hover:bg-emerald-200 dark:bg-emerald-900/40 dark:text-emerald-300 dark:hover:bg-emerald-800/60"
              >
                Accept all
              </button>
            ) : null;
          }}
          renderItem={(entry) => {
            const rel = entry.item as ImpliedRelationship;
            return (
              <label className="flex items-center gap-1.5 rounded border border-zinc-200 bg-white px-2 py-1 text-[10px] dark:border-zinc-800 dark:bg-zinc-950/60">
                <input
                  type="checkbox"
                  checked={!!confirmedRelationships[entry.key]}
                  onChange={(e) =>
                    setConfirmedRelationships((c) => ({ ...c, [entry.key]: e.target.checked }))
                  }
                />
                <span className="text-zinc-600 dark:text-zinc-300">
                  {rel.from_table}.{rel.from_column} → {rel.to_table}.{rel.to_column} ({Math.round(rel.confidence * 100)}%)
                </span>
              </label>
            );
          }}
        />
      )}

      {/* PII — grouped by table */}
      {report.pii_findings.length > 0 && (
        <GroupedSection
          title="PII Decisions"
          groups={piiGroups as Map<string, { key: string; item: unknown }[]>}
          searchFilter={searchFilter}
          unresolvedOnly={unresolvedOnly}
          getUnresolvedCount={piiUnresolvedByTable}
          renderBatchAction={(tableName) => {
            const unresolved = piiUnresolvedByTable(tableName);
            return unresolved > 0 ? (
              <button
                type="button"
                onClick={(e) => {
                  e.preventDefault();
                  acceptAllPiiInTable(tableName);
                }}
                className="rounded bg-emerald-100 px-1.5 py-0.5 text-[9px] font-medium text-emerald-700 hover:bg-emerald-200 dark:bg-emerald-900/40 dark:text-emerald-300 dark:hover:bg-emerald-800/60"
              >
                Accept all
              </button>
            ) : null;
          }}
          renderItem={(entry) => {
            const finding = entry.item as PiiFinding;
            return (
              <div className="rounded border border-zinc-200 bg-white p-2 dark:border-zinc-800 dark:bg-zinc-950/60">
                <p className="text-[10px] font-medium text-zinc-700 dark:text-zinc-200">
                  {finding.table}.{finding.column} ({finding.pii_type})
                </p>
                <select
                  value={piiDecisions[entry.key] ?? ""}
                  onChange={(e) =>
                    setPiiDecisions((c) => ({
                      ...c,
                      [entry.key]: e.target.value as PiiDecision | "",
                    }))
                  }
                  className={cn(selectClassName, "mt-1 !py-1 !text-xs")}
                >
                  <option value="">Choose...</option>
                  <option value="mask">Mask</option>
                  <option value="exclude">Exclude</option>
                  <option value="allow">Allow</option>
                </select>
              </div>
            );
          }}
        />
      )}

      {/* Clarifications — grouped by table */}
      {report.ambiguous_columns.length > 0 && (
        <GroupedSection
          title="Column Clarifications"
          groups={clarGroups as Map<string, { key: string; item: unknown }[]>}
          searchFilter={searchFilter}
          unresolvedOnly={unresolvedOnly}
          getUnresolvedCount={clarUnresolvedByTable}
          renderBatchAction={(tableName) => {
            const unresolved = clarUnresolvedByTable(tableName);
            return unresolved > 0 ? (
              <button
                type="button"
                onClick={(e) => {
                  e.preventDefault();
                  acceptAllClarInTable(tableName);
                }}
                className="rounded bg-emerald-100 px-1.5 py-0.5 text-[9px] font-medium text-emerald-700 hover:bg-emerald-200 dark:bg-emerald-900/40 dark:text-emerald-300 dark:hover:bg-emerald-800/60"
              >
                Accept all
              </button>
            ) : null;
          }}
          renderItem={(entry) => {
            const column = entry.item as AmbiguousColumn;
            return (
              <div className="rounded border border-zinc-200 bg-white p-2 dark:border-zinc-800 dark:bg-zinc-950/60">
                <p className="text-[10px] font-medium text-zinc-700 dark:text-zinc-200">
                  {column.table}.{column.column}
                </p>
                <p className="text-[10px] text-zinc-500">{column.clarification_prompt}</p>
                {column.repo_suggestion && (
                  <div className="mt-0.5 flex items-center gap-1.5">
                    <span className="text-[10px] text-emerald-600">{column.repo_suggestion.suggested_values}</span>
                    {!clarifications[entry.key]?.trim() && (
                      <button
                        onClick={() =>
                          setClarifications((c) => ({ ...c, [entry.key]: column.repo_suggestion!.suggested_values }))
                        }
                        className="rounded bg-emerald-100 px-1.5 py-0.5 text-[9px] font-medium text-emerald-700 hover:bg-emerald-200"
                      >
                        Accept
                      </button>
                    )}
                  </div>
                )}
                <FormInput
                  type="text"
                  placeholder="e.g. 0=draft, 1=active, 2=archived"
                  value={clarifications[entry.key] ?? ""}
                  onChange={(e) =>
                    setClarifications((c) => ({ ...c, [entry.key]: e.target.value }))
                  }
                  className="mt-1"
                />
              </div>
            );
          }}
        />
      )}

      {/* Excluded tables — grouped by table name */}
      {report.table_exclusion_suggestions.length > 0 && (
        <GroupedSection
          title="Excluded Tables"
          groups={excludedGroups as Map<string, { key: string; item: unknown }[]>}
          searchFilter={searchFilter}
          unresolvedOnly={unresolvedOnly}
          getUnresolvedCount={excludedUnresolvedByTable}
          renderBatchAction={(tableName) => {
            return !excludedTables[tableName] ? (
              <button
                type="button"
                onClick={(e) => {
                  e.preventDefault();
                  acceptAllExcludedInTable(tableName);
                }}
                className="rounded bg-emerald-100 px-1.5 py-0.5 text-[9px] font-medium text-emerald-700 hover:bg-emerald-200 dark:bg-emerald-900/40 dark:text-emerald-300 dark:hover:bg-emerald-800/60"
              >
                Accept all
              </button>
            ) : null;
          }}
          renderItem={(entry) => {
            const s = entry.item as TableExclusionSuggestion;
            return (
              <label className="flex items-center gap-1.5 rounded border border-zinc-200 bg-white px-2 py-1 text-[10px] dark:border-zinc-800 dark:bg-zinc-950/60">
                <input
                  type="checkbox"
                  checked={!!excludedTables[s.table_name]}
                  onChange={(e) =>
                    setExcludedTables((c) => ({ ...c, [s.table_name]: e.target.checked }))
                  }
                />
                <span className="text-zinc-600 dark:text-zinc-300">
                  {s.table_name} ({s.reason}
                  {typeof s.row_count === "number" ? ` — ${s.row_count} rows` : ""})
                </span>
              </label>
            );
          }}
        />
      )}
    </div>
  );
}
