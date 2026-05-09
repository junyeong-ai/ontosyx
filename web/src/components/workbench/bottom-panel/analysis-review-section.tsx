"use client";

import { useCallback, useMemo, useState } from "react";
import { useTranslations } from "next-intl";
import { AlertTriangle, Wand2 } from "lucide-react";
import { Eyebrow } from "@/components/ui/eyebrow";
import { FormInput, SettingsSelect } from "@/components/ui/form-input";
import { Checkbox } from "@/components/ui/checkbox";
import { cn } from "@/lib/cn";
import { toast } from "@/components/ui/toast";
import { WarningGroupList } from "@/components/workbench/warnings/warning-group-card";
import { ReviewToc, type ReviewTOCEntry } from "./review-toc";
import { useReviewKeyboardNav } from "./use-review-keyboard-nav";
import type {
  AmbiguityContext,
  ImpliedRelationship,
  PiiKind,
  PiiSuggestion,
  SourceAnalysisReport,
  TableExclusionSuggestion,
} from "@/types/api";
import {
  relationshipKey,
  columnKey,
} from "./design-panel-shared";
import type { PiiAnnotationEntry } from "./use-design-decisions";

type AnalysisTranslator = ReturnType<typeof useTranslations<"workbench.bottomPanel.analysisReview">>;

// ---------------------------------------------------------------------------
// PII kind picker — every variant the operator can commit. The form
// surfaces the discriminant only; structural variants (`national_id`,
// `custom`) carry an empty payload by default and are refined in the
// dedicated annotation editor.
// ---------------------------------------------------------------------------

const PII_KIND_VALUES: { value: string; build: () => PiiKind }[] = [
  { value: "name", build: () => ({ kind: "name" }) },
  { value: "date_of_birth", build: () => ({ kind: "date_of_birth" }) },
  { value: "national_id", build: () => ({ kind: "national_id", value: { country: "" } }) },
  { value: "passport", build: () => ({ kind: "passport" }) },
  { value: "drivers_license", build: () => ({ kind: "drivers_license" }) },
  { value: "email", build: () => ({ kind: "email" }) },
  { value: "phone", build: () => ({ kind: "phone" }) },
  { value: "address", build: () => ({ kind: "address" }) },
  { value: "ip_address", build: () => ({ kind: "ip_address" }) },
  { value: "payment_card_number", build: () => ({ kind: "payment_card_number" }) },
  { value: "bank_account_number", build: () => ({ kind: "bank_account_number" }) },
  { value: "iban", build: () => ({ kind: "iban" }) },
  { value: "credit_card", build: () => ({ kind: "credit_card" }) },
  { value: "ssn", build: () => ({ kind: "ssn" }) },
  { value: "medical_record_number", build: () => ({ kind: "medical_record_number" }) },
  { value: "insurance_id", build: () => ({ kind: "insurance_id" }) },
  { value: "biometric", build: () => ({ kind: "biometric" }) },
  { value: "geo_location", build: () => ({ kind: "geo_location" }) },
  { value: "password", build: () => ({ kind: "password" }) },
  { value: "token", build: () => ({ kind: "token" }) },
  { value: "custom", build: () => ({ kind: "custom", value: "" }) },
];

function piiKindFromValue(value: string): PiiKind | undefined {
  return PII_KIND_VALUES.find((entry) => entry.value === value)?.build();
}

// ---------------------------------------------------------------------------
// Column clarification auto-fill heuristics
// ---------------------------------------------------------------------------

function inferClarification(column: AmbiguityContext, t: AnalysisTranslator): string {
  const col = column.column.column.toLowerCase();
  const samples = column.sample_values ?? [];

  if (/year/.test(col) && samples.every((v) => /^\d{4}$/.test(v.trim()))) {
    return t("clarHintDefault");
  }
  if (/age/.test(col) && samples.every((v) => /^\d{1,3}$/.test(v.trim()))) {
    return t("inferAge");
  }
  if (/pct|percent/.test(col)) {
    return t("inferPercentage");
  }
  if (/rating/.test(col) && samples.every((v) => /^\d{1,2}$/.test(v.trim()))) {
    const nums = samples.map((v) => Number(v.trim())).filter((n) => !Number.isNaN(n));
    if (nums.length > 0) {
      return t("inferRatingRange", { min: Math.min(...nums), max: Math.max(...nums) });
    }
    return t("inferRating");
  }
  if (/grade/.test(col)) {
    return t("inferGrade");
  }
  if (/quantity|qty/.test(col)) {
    return t("inferQuantity");
  }
  if (/type|status|category|kind/.test(col) && samples.length > 0) {
    return t("inferCategory", { values: samples.join(", ") });
  }
  const readable = col
    .replace(/_/g, " ")
    .replace(/\b\w/g, (c) => c.toUpperCase());
  if (samples.length > 0) {
    return t("inferReadable", { name: readable, values: samples.slice(0, 5).join(", ") });
  }
  return readable;
}

// ---------------------------------------------------------------------------
// Grouping + section helpers
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
  const t = useTranslations("workbench.bottomPanel.analysisReview");
  if (groups.size === 0) return null;

  const lowerSearch = searchFilter.toLowerCase();
  const filteredGroups = Array.from(groups.entries())
    .filter(([tableName]) => !lowerSearch || tableName.toLowerCase().includes(lowerSearch))
    .filter(([tableName]) => !unresolvedOnly || getUnresolvedCount(tableName) > 0)
    .sort(([a], [b]) => a.localeCompare(b));

  // Every group has unresolved=0 (filtered out by `unresolvedOnly`),
  // or no group matches the text filter. Render a compact "all
  // resolved" placeholder so the section still has visible content
  // when the TOC pill scrolls here. Without this the wrapper div
  // exists but is empty, and the click-to-anchor ring highlight
  // flashes against a blank box — confusing the operator into
  // thinking the click did nothing.
  if (filteredGroups.length === 0) {
    const totalItems = Array.from(groups.values()).reduce((acc, list) => acc + list.length, 0);
    return (
      <div>
        <Eyebrow level={4} size="dense" className="mb-1">
          {title}
        </Eyebrow>
        <p className="rounded border border-brand-border bg-brand-surface px-2 py-1.5 text-xs text-brand-foreground-strong">
          {t("sectionAllResolved", { count: totalItems })}
        </p>
      </div>
    );
  }

  return (
    <div>
      <Eyebrow level={4} tone="muted" size="dense" caps="upper" className="mb-1">
        {title}
      </Eyebrow>
      <div className="space-y-1">
        {filteredGroups.map(([tableName, entries]) => {
          const unresolved = getUnresolvedCount(tableName);
          return (
            <details key={tableName} open={unresolved > 0}>
              <summary className="flex cursor-pointer select-none items-center gap-2 rounded border border-divider bg-surface-inset px-2 py-1 text-xs font-medium text-foreground hover:bg-surface-inset">
                <span className="flex-1">{tableName}</span>
                {renderBatchAction?.(tableName)}
                {unresolved > 0 && (
                  <span className="rounded-full bg-warning-surface px-1.5 py-0.5 text-2xs font-medium text-warning-foreground">
                    {unresolved}
                  </span>
                )}
              </summary>
              <div className="mt-1 space-y-1 ps-2">
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
// Analysis Review Section
// ---------------------------------------------------------------------------

export function AnalysisReviewSection({
  report,
  confirmedRelationships,
  setConfirmedRelationships,
  piiAnnotations,
  setPiiAnnotations,
  excludedColumns,
  setExcludedColumns,
  clarifications,
  setClarifications,
  excludedTables,
  setExcludedTables,
  partialAnalysisAcknowledged,
  setPartialAnalysisAcknowledged,
  largeSchemaAcknowledged,
  setLargeSchemaAcknowledged,
  unresolvedClarificationCount,
}: {
  report: SourceAnalysisReport;
  confirmedRelationships: Record<string, boolean>;
  setConfirmedRelationships: React.Dispatch<
    React.SetStateAction<Record<string, boolean>>
  >;
  piiAnnotations: Record<string, PiiAnnotationEntry>;
  setPiiAnnotations: React.Dispatch<
    React.SetStateAction<Record<string, PiiAnnotationEntry>>
  >;
  excludedColumns: Record<string, { table: string; column: string }>;
  setExcludedColumns: React.Dispatch<
    React.SetStateAction<Record<string, { table: string; column: string }>>
  >;
  clarifications: Record<string, string>;
  setClarifications: React.Dispatch<
    React.SetStateAction<Record<string, string>>
  >;
  excludedTables: Record<string, boolean>;
  setExcludedTables: React.Dispatch<
    React.SetStateAction<Record<string, boolean>>
  >;
  partialAnalysisAcknowledged: boolean;
  setPartialAnalysisAcknowledged: (v: boolean) => void;
  largeSchemaAcknowledged: boolean;
  setLargeSchemaAcknowledged: (v: boolean) => void;
  unresolvedClarificationCount: number;
}) {
  const t = useTranslations("workbench.bottomPanel.analysisReview");
  const [searchFilter, setSearchFilter] = useState("");
  const [unresolvedOnly, setUnresolvedOnly] = useState(true);

  const totalItems = useMemo(() => {
    return (
      report.implied_relationships.length +
      report.pii_suggestions.length +
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

  const unresolvedPiiCount = useMemo(() => {
    return report.pii_suggestions.filter((s) => {
      const key = columnKey(s.table, s.column);
      return !piiAnnotations[key] && !excludedColumns[key];
    }).length;
  }, [report.pii_suggestions, piiAnnotations, excludedColumns]);

  const unresolvedExcludedCount = useMemo(() => {
    return report.table_exclusion_suggestions.filter(
      (s) => !excludedTables[s.table_name],
    ).length;
  }, [report.table_exclusion_suggestions, excludedTables]);

  const totalUnresolved =
    unresolvedRelCount +
    unresolvedPiiCount +
    unresolvedClarificationCount +
    unresolvedExcludedCount;
  const totalResolved = totalItems - totalUnresolved;
  const progressPercent = totalItems > 0 ? Math.round((totalResolved / totalItems) * 100) : 100;
  const analysisWarnings = report.analysis_warnings ?? [];

  // Sticky-TOC backing data. Keep this declarative so adding a new
  // review section is one entry here + one anchor id on the
  // rendered <section>. Sections with zero items are filtered
  // inside `<ReviewToc>` and do not produce TOC pills.
  const tocEntries: ReviewTOCEntry[] = [
    {
      anchor: "review-warnings",
      labelKey: "warnings",
      total: analysisWarnings.length,
      unresolved: partialAnalysisAcknowledged ? 0 : analysisWarnings.length,
    },
    {
      anchor: "review-relationships",
      labelKey: "relationships",
      total: report.implied_relationships.length,
      unresolved: unresolvedRelCount,
    },
    {
      anchor: "review-exclusions",
      labelKey: "exclusions",
      total: report.table_exclusion_suggestions.length,
      unresolved: unresolvedExcludedCount,
    },
    {
      anchor: "review-pii",
      labelKey: "pii",
      total: report.pii_suggestions.length,
      unresolved: unresolvedPiiCount,
    },
    {
      anchor: "review-clarifications",
      labelKey: "clarifications",
      total: report.ambiguous_columns.length,
      unresolved: unresolvedClarificationCount,
    },
  ];

  // J/K keyboard nav across the same anchors the TOC pills jump to.
  // The hook filters out anchors whose underlying element is missing
  // (section hidden because the data slice is empty), so the cursor
  // never strands on a non-existent target.
  useReviewKeyboardNav(tocEntries.map((entry) => entry.anchor));

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
    const grouped = groupByTable(report.pii_suggestions, (s) => s.table);
    const result = new Map<string, { key: string; item: PiiSuggestion }[]>();
    for (const [table, items] of grouped) {
      result.set(
        table,
        items.map((s) => ({ key: columnKey(s.table, s.column), item: s })),
      );
    }
    return result;
  }, [report.pii_suggestions]);

  const clarGroups = useMemo(() => {
    const grouped = groupByTable(report.ambiguous_columns, (c) => c.column.relation);
    const result = new Map<string, { key: string; item: AmbiguityContext }[]>();
    for (const [table, items] of grouped) {
      result.set(
        table,
        items.map((c) => ({
          key: columnKey(c.column.relation, c.column.column),
          item: c,
        })),
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
      return items.filter((e) => !piiAnnotations[e.key] && !excludedColumns[e.key]).length;
    },
    [piiGroups, piiAnnotations, excludedColumns],
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
      const updates: Record<string, PiiAnnotationEntry> = {};
      for (const e of items) {
        if (!piiAnnotations[e.key] && !excludedColumns[e.key]) {
          const suggestion = e.item as PiiSuggestion;
          updates[e.key] = {
            table: suggestion.table,
            column: suggestion.column,
            kind: suggestion.kind,
          };
        }
      }
      if (Object.keys(updates).length > 0) {
        setPiiAnnotations((prev) => ({ ...prev, ...updates }));
      }
    },
    [piiGroups, piiAnnotations, excludedColumns, setPiiAnnotations],
  );

  const acceptAllClarInTable = useCallback(
    (tableName: string) => {
      const items = clarGroups.get(tableName);
      if (!items) return;
      const updates: Record<string, string> = {};
      for (const e of items) {
        if (!clarifications[e.key]?.trim()) {
          const col = e.item as AmbiguityContext;
          updates[e.key] = col.repo_hint
            ? col.repo_hint.suggested_values
            : inferClarification(col, t);
        }
      }
      if (Object.keys(updates).length > 0) {
        setClarifications((prev) => ({ ...prev, ...updates }));
      }
    },
    [clarGroups, clarifications, setClarifications, t],
  );

  const acceptAllExcludedInTable = useCallback(
    (tableName: string) => {
      setExcludedTables((prev) => ({ ...prev, [tableName]: true }));
    },
    [setExcludedTables],
  );

  const autoFill = useCallback(() => {
    let piiCount = 0;
    let clarCount = 0;

    if (report.pii_suggestions.length > 0) {
      const newPii: Record<string, PiiAnnotationEntry> = {};
      for (const suggestion of report.pii_suggestions) {
        const key = columnKey(suggestion.table, suggestion.column);
        if (!piiAnnotations[key] && !excludedColumns[key]) {
          newPii[key] = {
            table: suggestion.table,
            column: suggestion.column,
            kind: suggestion.kind,
          };
          piiCount += 1;
        }
      }
      if (piiCount > 0) {
        setPiiAnnotations((prev) => ({ ...prev, ...newPii }));
      }
    }

    if (report.ambiguous_columns.length > 0) {
      const newClar: Record<string, string> = {};
      for (const column of report.ambiguous_columns) {
        const key = columnKey(column.column.relation, column.column.column);
        if (!clarifications[key]?.trim()) {
          if (column.repo_hint) {
            newClar[key] = column.repo_hint.suggested_values;
          } else {
            newClar[key] = inferClarification(column, t);
          }
          clarCount += 1;
        }
      }
      if (clarCount > 0) {
        setClarifications((prev) => ({ ...prev, ...newClar }));
      }
    }

    if (piiCount === 0 && clarCount === 0) {
      toast.info(t("autoFillComplete"));
    } else {
      toast.success(
        t("autoFillSuccess", {
          pii: piiCount,
          piiLabel: piiCount !== 1 ? t("autoFillPiiPlural") : t("autoFillPiiSingular"),
          clar: clarCount,
          clarLabel: clarCount !== 1 ? t("autoFillClarPlural") : t("autoFillClarSingular"),
        }),
        { description: t("autoFillDescription") },
      );
    }
  }, [
    report,
    piiAnnotations,
    excludedColumns,
    setPiiAnnotations,
    clarifications,
    setClarifications,
    t,
  ]);

  const hasUnresolved = unresolvedPiiCount > 0 || unresolvedClarificationCount > 0;

  return (
    <div className="space-y-3 rounded-lg border border-divider bg-surface-raised p-3">
      <div className="flex items-center justify-between gap-3">
        <div>
          <p className="text-xs font-semibold text-foreground-strong">
            {t("heading")}
          </p>
          <p className="mt-0.5 text-xs text-foreground-muted">
            {t("description", {
              tables: report.schema_stats.table_count,
              columns: report.schema_stats.column_count,
              fks: report.schema_stats.declared_fk_count,
              pii: unresolvedPiiCount,
              clarifications: unresolvedClarificationCount,
            })}
          </p>
        </div>
        <div className="flex items-center gap-2">
          {hasUnresolved && (
            <button
              type="button"
              onClick={autoFill}
              className={cn(
                "flex items-center gap-1 rounded-md border px-2 py-1 text-xs font-medium transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)]",
                "border-concept-border bg-concept-surface text-concept-foreground hover:bg-concept-surface",
              )}
            >
              <Wand2 className="h-3 w-3" />
              {t("autoFill")}
            </button>
          )}
          <span
            className={cn(
              "rounded-full px-1.5 py-0.5 text-2xs font-medium uppercase",
              report.analysis_completeness === "partial"
                ? "bg-warning-surface text-warning-foreground"
                : "bg-brand-surface-strong text-brand-foreground-strong",
            )}
          >
            {report.analysis_completeness === "partial"
              ? t("completenessPartial")
              : t("completenessFull")}
          </span>
        </div>
      </div>

      {totalItems > 0 && (
        <div>
          <div className="mb-1 flex items-center justify-between text-xs text-foreground-muted">
            <span>{t("progressResolved", { percent: progressPercent, resolved: totalResolved, total: totalItems })}</span>
            <span className="text-foreground-muted">{t("progressRemaining", { count: totalUnresolved })}</span>
          </div>
          <div className="h-1.5 w-full overflow-hidden rounded-full bg-surface-inset">
            <div
              className={cn(
                "h-full rounded-full transition-all duration-[var(--duration-slow)] ease-[var(--ease-out)]",
                progressPercent === 100
                  ? "bg-brand-solid"
                  : progressPercent >= 50
                    ? "bg-brand-solid"
                    : "bg-warning-foreground",
              )}
              style={{ width: `${progressPercent}%` }}
            />
          </div>
        </div>
      )}

      <ReviewToc entries={tocEntries} />

      {totalItems > 0 && (
        <div className="flex flex-wrap items-center gap-2 rounded-md border border-divider bg-surface-base px-2 py-1.5">
          <Checkbox
            checked={unresolvedOnly}
            onChange={(e) => setUnresolvedOnly(e.target.checked)}
            label={t("unresolvedOnly")}
          />
          <div className="h-3 w-px bg-surface-inset" />
          <FormInput
            type="text"
            value={searchFilter}
            onChange={(e) => setSearchFilter(e.target.value)}
            placeholder={t("filterPlaceholder")}
            className="flex-1 border-none bg-transparent focus:ring-0"
          />
          <span className="whitespace-nowrap text-xs font-medium text-foreground-muted">
            {t("filterCount", { remaining: totalUnresolved, total: totalItems })}
          </span>
        </div>
      )}

      {analysisWarnings.length > 0 && (
        <div
          id="review-warnings"
          className="rounded-md border border-warning-border bg-warning-surface p-2"
        >
          <div className="flex items-center gap-1.5">
            <AlertTriangle className="h-3 w-3 text-warning-foreground" />
            <span className="text-xs font-medium text-warning-foreground">
              {t("warningsTitle")}
            </span>
          </div>
          <div className="mt-2">
            <WarningGroupList warnings={analysisWarnings} />
          </div>
          <Checkbox
            id="review-partial-acknowledgement"
            checked={partialAnalysisAcknowledged}
            onChange={(e) => setPartialAnalysisAcknowledged(e.target.checked)}
            align="start"
            label={<span className="font-medium">{t("acknowledgePartial")}</span>}
            className="mt-2"
          />
        </div>
      )}

      {report.large_schema_warning && (
        <div
          id="review-large-schema"
          className="rounded-md border border-warning-border bg-warning-surface p-2"
        >
          <div className="flex items-center gap-1.5">
            <AlertTriangle className="h-3 w-3 text-warning-foreground" />
            <span className="text-xs font-medium text-warning-foreground">
              {t("largeSchemaTitle", {
                tableCount: report.large_schema_warning.table_count,
                recommendedMax: report.large_schema_warning.recommended_max,
              })}
            </span>
          </div>
          <p className="mt-1 text-xs text-warning-foreground">
            {t("largeSchemaHint")}
          </p>
          <Checkbox
            id="review-large-schema-acknowledgement"
            checked={largeSchemaAcknowledged}
            onChange={(e) => setLargeSchemaAcknowledged(e.target.checked)}
            align="start"
            label={<span className="font-medium">{t("acknowledgeLargeSchema")}</span>}
            className="mt-2"
          />
        </div>
      )}

      {report.repo_summary && (
        <div className="space-y-1 text-xs text-foreground-muted">
          <p>
            {t("repoSummary", {
              status: report.repo_summary.status,
              analyzed: report.repo_summary.files_analyzed,
              requested: report.repo_summary.files_requested,
            })}
            {report.repo_summary.enums_found > 0 &&
              t("repoEnums", { count: report.repo_summary.enums_found })}
          </p>
          {report.repo_summary.failure_reason && (
            <p className="text-warning-foreground">
              {t(`repoFailure.${report.repo_summary.failure_reason}`)}
            </p>
          )}
        </div>
      )}

      {/* Relationships */}
      {report.implied_relationships.length > 0 && (
        <div id="review-relationships">
        <GroupedSection
          title={t("confirmRelationships")}
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
                className="rounded bg-brand-surface-strong px-1.5 py-0.5 text-2xs font-medium text-brand-foreground hover:bg-brand-surface-strong/60"
              >
                {t("acceptAll")}
              </button>
            ) : null;
          }}
          renderItem={(entry) => {
            const rel = entry.item as ImpliedRelationship;
            return (
              <Checkbox
                checked={!!confirmedRelationships[entry.key]}
                onChange={(e) =>
                  setConfirmedRelationships((c) => ({ ...c, [entry.key]: e.target.checked }))
                }
                label={
                  <span className="text-foreground-muted">
                    {t("relationshipRow", {
                      fromTable: rel.from_table,
                      fromColumn: rel.from_column,
                      toTable: rel.to_table,
                      toColumn: rel.to_column,
                      confidence: Math.round(rel.confidence * 100),
                    })}
                  </span>
                }
                className="rounded border border-divider bg-surface-base px-2 py-1"
              />
            );
          }}
        />
        </div>
      )}

      {/* PII suggestions */}
      {report.pii_suggestions.length > 0 && (
        <div id="review-pii">
        <GroupedSection
          title={t("piiDecisions")}
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
                className="rounded bg-brand-surface-strong px-1.5 py-0.5 text-2xs font-medium text-brand-foreground hover:bg-brand-surface-strong/60"
              >
                {t("acceptAll")}
              </button>
            ) : null;
          }}
          renderItem={(entry) => {
            const suggestion = entry.item as PiiSuggestion;
            const annotation = piiAnnotations[entry.key];
            const excluded = !!excludedColumns[entry.key];
            const selectedValue = annotation ? annotation.kind.kind : "";
            return (
              <div className="rounded border border-divider bg-surface-base p-2">
                <p className="text-xs font-medium text-foreground-strong">
                  {t("piiRow", {
                    table: suggestion.table,
                    column: suggestion.column,
                    type: suggestion.kind.kind,
                  })}
                </p>
                <p className="text-xs text-foreground-muted">
                  {Math.round(suggestion.confidence * 100)}% — {suggestion.reason}
                </p>
                <div className="mt-1 flex items-center gap-2">
                  <SettingsSelect
                    label={t("piiOptionChoose")}
                    hideLabel
                    value={selectedValue}
                    onChange={(e) => {
                      const value = e.target.value;
                      if (!value) {
                        setPiiAnnotations((c) => {
                          const next = { ...c };
                          delete next[entry.key];
                          return next;
                        });
                        return;
                      }
                      const kind = piiKindFromValue(value);
                      if (!kind) return;
                      setPiiAnnotations((c) => ({
                        ...c,
                        [entry.key]: {
                          table: suggestion.table,
                          column: suggestion.column,
                          kind,
                        },
                      }));
                      setExcludedColumns((c) => {
                        if (!c[entry.key]) return c;
                        const next = { ...c };
                        delete next[entry.key];
                        return next;
                      });
                    }}
                    disabled={excluded}
                  >
                    <option value="">{t("piiOptionChoose")}</option>
                    {PII_KIND_VALUES.map((entry) => (
                      <option key={entry.value} value={entry.value}>
                        {entry.value}
                      </option>
                    ))}
                  </SettingsSelect>
                  <Checkbox
                    checked={excluded}
                    onChange={(e) => {
                      if (e.target.checked) {
                        setExcludedColumns((c) => ({
                          ...c,
                          [entry.key]: {
                            table: suggestion.table,
                            column: suggestion.column,
                          },
                        }));
                        setPiiAnnotations((c) => {
                          if (!c[entry.key]) return c;
                          const next = { ...c };
                          delete next[entry.key];
                          return next;
                        });
                      } else {
                        setExcludedColumns((c) => {
                          const next = { ...c };
                          delete next[entry.key];
                          return next;
                        });
                      }
                    }}
                    label={t("piiOptionExclude")}
                  />
                </div>
              </div>
            );
          }}
        />
        </div>
      )}

      {/* PII close + Clarifications open */}
      {/* Clarifications */}
      {report.ambiguous_columns.length > 0 && (
        <div id="review-clarifications">
        <GroupedSection
          title={t("columnClarifications")}
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
                className="rounded bg-brand-surface-strong px-1.5 py-0.5 text-2xs font-medium text-brand-foreground hover:bg-brand-surface-strong/60"
              >
                {t("acceptAll")}
              </button>
            ) : null;
          }}
          renderItem={(entry) => {
            const column = entry.item as AmbiguityContext;
            return (
              <div className="rounded border border-divider bg-surface-base p-2">
                <p className="text-xs font-medium text-foreground-strong">
                  {t("clarificationRowHeader", {
                    table: column.column.relation,
                    column: column.column.column,
                  })}
                </p>
                <p className="text-xs text-foreground-muted">{column.clarification_prompt}</p>
                {column.repo_hint && (
                  <div className="mt-0.5 flex items-center gap-1.5">
                    <span className="text-xs text-brand-foreground">{column.repo_hint.suggested_values}</span>
                    {!clarifications[entry.key]?.trim() && (
                      <button type="button"
                        onClick={() =>
                          setClarifications((c) => ({
                            ...c,
                            [entry.key]: column.repo_hint!.suggested_values,
                          }))
                        }
                        className="rounded bg-brand-surface-strong px-1.5 py-0.5 text-2xs font-medium text-brand-foreground hover:bg-brand-surface-strong"
                      >
                        {t("clarificationAccept")}
                      </button>
                    )}
                  </div>
                )}
                <FormInput
                  type="text"
                  placeholder={t("clarificationPlaceholder")}
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
        </div>
      )}

      {/* Excluded tables */}
      {report.table_exclusion_suggestions.length > 0 && (
        <div id="review-exclusions">
        <GroupedSection
          title={t("excludedTables")}
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
                className="rounded bg-brand-surface-strong px-1.5 py-0.5 text-2xs font-medium text-brand-foreground hover:bg-brand-surface-strong/60"
              >
                {t("acceptAll")}
              </button>
            ) : null;
          }}
          renderItem={(entry) => {
            const s = entry.item as TableExclusionSuggestion;
            return (
              <Checkbox
                checked={!!excludedTables[s.table_name]}
                onChange={(e) =>
                  setExcludedTables((c) => ({ ...c, [s.table_name]: e.target.checked }))
                }
                label={
                  <span className="text-foreground-muted">
                    {t("excludedRow", {
                      table: s.table_name,
                      reason: s.reason,
                      rows:
                        typeof s.row_count === "number"
                          ? t("excludedRows", { count: s.row_count })
                          : "",
                    })}
                  </span>
                }
                className="rounded border border-divider bg-surface-base px-2 py-1"
              />
            );
          }}
        />
        </div>
      )}
    </div>
  );
}
