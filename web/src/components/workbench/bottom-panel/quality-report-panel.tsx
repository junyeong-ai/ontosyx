"use client";

import { useState, useMemo, useCallback } from "react";
import { useTranslations } from "next-intl";
import { cn } from "@/lib/cn";
import { Wand2 } from "lucide-react";
import { useConfirm } from "@/components/providers/confirm-provider";
import { toast } from "@/components/ui/toast";
import { FormInput } from "@/components/ui/form-input";
import type {
  OntologyQualityReport,
  QualityGap,
  QualityGapSeverity,
  QualityGapCategory,
  ColumnClarification,
} from "@/types/api";
import { formatGapLocation } from "./design-panel-shared";
import { getGapEntityId } from "@/lib/quality-utils";
import { gapToEditRequest } from "@/lib/gap-to-edit-request";
import {
  localizeQualityGapIssue,
  localizeQualityGapSuggestion,
} from "@/lib/quality-gap-text";
import { updateDecisions, getOntologyDraft } from "@/lib/api";
import { useAppStore } from "@/lib/store";
import { QualityGapCard, AI_FIXABLE_CATEGORIES } from "./quality-gap-card";

type QualityTranslator = ReturnType<typeof useTranslations<"workbench.bottomPanel.quality">>;
type QualityGapTranslator = ReturnType<typeof useTranslations<"qualityGap">>;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const SEVERITY_ORDER: Record<QualityGapSeverity, number> = {
  high: 0,
  medium: 1,
  low: 2,
};

// Known quality gap categories mirrored from the backend wire type.
// Unknown variants fall back to the raw snake_case value.
const KNOWN_CATEGORIES = [
  "opaque_enum_value",
  "numeric_enum_code",
  "single_value_bias",
  "small_sample",
  "missing_description",
  "sparse_property",
  "unmapped_source_table",
  "missing_foreign_key_edge",
  "missing_containment_edge",
  "unmapped_source_column",
  "duplicate_edge",
  "orphan_node",
  "property_type_inconsistency",
  "hub_node",
  "overloaded_property",
  "self_referential_edge",
] as const;
type KnownCategory = (typeof KNOWN_CATEGORIES)[number];
function isKnownCategory(s: string): s is KnownCategory {
  return (KNOWN_CATEGORIES as readonly string[]).includes(s);
}

function formatCategory(
  category: QualityGapCategory,
  t: QualityTranslator,
): string {
  // Widen to plain `string` so TS doesn't narrow to `never` after the
  // `isKnownCategory` guard — the wire type is the closed union mirror,
  // but the guard is written defensively for forward-compat.
  const raw: string = category;
  if (isKnownCategory(raw)) return t(`categories.${raw}`);
  return raw
    .split("_")
    .map((w) => w.charAt(0).toUpperCase() + w.slice(1))
    .join(" ");
}

function countBadgeClass(severity: QualityGapSeverity): string {
  return cn(
    "rounded-full px-1.5 py-0.5 text-2xs font-medium tabular-nums",
    severity === "high"
      ? "bg-danger-surface text-danger-foreground"
      : severity === "medium"
        ? "bg-warning-surface text-warning-foreground"
        : "bg-surface-inset text-foreground",
  );
}

/**
 * Build an acknowledgment clarification hint based on gap type and location.
 * Accepts a translator so the hint matches the user's active locale — the
 * hint is persisted verbatim, so it should render in the UI language.
 */
function buildAcknowledgmentHint(
  gap: QualityGap,
  t: QualityTranslator,
  tGap: QualityGapTranslator,
): string {
  if (gap.category === "single_value_bias") {
    const value = gap.params.observed_value ?? t("acknowledgmentHintSingleValueDefault");
    return t("acknowledgmentHintSingleValue", { value });
  }
  if (gap.category === "sparse_property") {
    return t("acknowledgmentHintSparse");
  }
  // Fallback for any other gap categories the UI is asked to acknowledge —
  // surface the localized issue so the persisted hint stays meaningful.
  return t("acknowledgmentHintGeneric", { issue: localizeQualityGapIssue(gap, tGap) });
}

/**
 * Extract source table and column from a gap location.
 * For node_property / edge_property gaps, we need to look up the source mapping.
 * For source_column gaps, it's directly available.
 */
function extractSourceLocation(gap: QualityGap): { table: string; column: string } | null {
  const loc = gap.location;
  if (loc.ref_type === "source_column") {
    return { table: loc.table, column: loc.column };
  }
  // For node_property / edge_property, try to extract from the issue/suggestion text
  // The gap issue/suggestion typically mentions the source column
  if (loc.ref_type === "node_property" || loc.ref_type === "edge_property") {
    // The quality assessment creates gaps with node_property ref_type,
    // and the source table is the node's source_table. We need to extract
    // table.column from the gap itself. The property_name usually maps to the
    // source column name, and the label maps to the source table.
    // This is an approximation -- the source_table on the node is used.
    return { table: loc.label, column: loc.property_name };
  }
  return null;
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

interface QualityReportPanelProps {
  report: OntologyQualityReport;
}

export function QualityReportPanel({ report }: QualityReportPanelProps) {
  const t = useTranslations("workbench.bottomPanel.quality");
  const tGap = useTranslations("qualityGap");
  // Workflow heading used to locate the Analysis Review <details> element
  // when navigating from a gap to its clarification source.
  const analysisReviewHeading = useTranslations(
    "workbench.bottomPanel.workflow",
  )("analysisReview");
  const [enabledSeverities, setEnabledSeverities] = useState<
    Set<QualityGapSeverity>
  >(new Set(["high", "medium", "low"]));
  const [collapsedCategories, setCollapsedCategories] = useState<Set<string>>(
    new Set(),
  );
  const [acknowledgingIndex, setAcknowledgingIndex] = useState<number | null>(null);
  const [searchQuery, setSearchQuery] = useState("");

  const activeOntologyDraft = useAppStore((s) => s.activeOntologyDraft);
  const setActiveOntologyDraft = useAppStore((s) => s.setActiveOntologyDraft);
  const setCommandBarInput = useAppStore((s) => s.setCommandBarInput);
  const setDesignBottomTab = useAppStore((s) => s.setDesignBottomTab);

  const fixGap = useCallback(
    (gap: QualityGap) => {
      setCommandBarInput(gapToEditRequest(gap, tGap));
    },
    [setCommandBarInput, tGap],
  );

  const confirmDialog = useConfirm();

  const acknowledgeGap = useCallback(
    async (gap: QualityGap, index: number) => {
      if (!activeOntologyDraft) return;

      const sourceLoc = extractSourceLocation(gap);
      if (!sourceLoc) {
        toast.error(t("cannotDetermineSource"));
        return;
      }

      // Confirm with user before acknowledging
      const confirmed = await confirmDialog({
        title: t("acknowledgeTitle"),
        description: gap.category === "single_value_bias"
          ? t("acknowledgeSingleValueDescription", { table: sourceLoc.table, column: sourceLoc.column })
          : t("acknowledgeSparseDescription", { table: sourceLoc.table, column: sourceLoc.column }),
        confirmLabel: t("acknowledgeConfirmLabel"),
      });
      if (!confirmed) return;

      setAcknowledgingIndex(index);
      try {
        const hint = buildAcknowledgmentHint(gap, t, tGap);
        const existingClarifications = activeOntologyDraft.design_options.column_clarifications ?? [];

        // Check if a clarification already exists for this column
        const alreadyExists = existingClarifications.some(
          (c) =>
            c.table.toLowerCase() === sourceLoc.table.toLowerCase() &&
            c.column.toLowerCase() === sourceLoc.column.toLowerCase(),
        );

        let newClarifications: ColumnClarification[];
        if (alreadyExists) {
          // Update existing clarification
          newClarifications = existingClarifications.map((c) =>
            c.table.toLowerCase() === sourceLoc.table.toLowerCase() &&
            c.column.toLowerCase() === sourceLoc.column.toLowerCase()
              ? { ...c, hint }
              : c,
          );
        } else {
          newClarifications = [
            ...existingClarifications,
            { table: sourceLoc.table, column: sourceLoc.column, hint },
          ];
        }

        const updatedDraft = await updateDecisions(activeOntologyDraft.id, {
          design_options: {
            ...activeOntologyDraft.design_options,
            column_clarifications: newClarifications,
          },
          revision: activeOntologyDraft.revision,
        });
        setActiveOntologyDraft(updatedDraft);
        toast.success(t("gapAcknowledged"), {
          description: t("clarificationAdded", { table: sourceLoc.table, column: sourceLoc.column }),
        });
      } catch (err) {
        toast.error(t("acknowledgeFailed"), {
          description: err instanceof Error ? err.message : "Unknown error",
        });
        // Try to reload project in case of conflict
        try {
          const fresh = await getOntologyDraft(activeOntologyDraft.id);
          setActiveOntologyDraft(fresh);
        } catch {
          /* ignore reload failure */
        }
      } finally {
        setAcknowledgingIndex(null);
      }
    },
    [activeOntologyDraft, setActiveOntologyDraft, confirmDialog, t, tGap],
  );

  const navigateToClarification = useCallback(
    (gap: QualityGap) => {
      const sourceLoc = extractSourceLocation(gap);
      const locationLabel = sourceLoc
        ? `${sourceLoc.table}.${sourceLoc.column}`
        : formatGapLocation(gap.location);

      // Switch to Workflow tab and scroll to the Analysis Review section
      setDesignBottomTab("workflow");

      // Open the Analysis Review <details> element and scroll to it
      requestAnimationFrame(() => {
        const detailElements = document.querySelectorAll<HTMLDetailsElement>("details");
        for (const d of detailElements) {
          const summary = d.querySelector("summary");
          if (summary?.textContent?.includes(analysisReviewHeading)) {
            d.open = true;
            d.scrollIntoView({ behavior: "smooth", block: "start" });
            break;
          }
        }
      });

      toast.info(t("navigationHint", { location: locationLabel }), {
        description: t("navigationDescription"),
      });
    },
    [setDesignBottomTab, t, analysisReviewHeading],
  );

  const fixAll = useCallback(() => {
    const fixableGaps = report.gaps.filter(
      (g) => getGapEntityId(g) !== null && AI_FIXABLE_CATEGORIES.has(g.category),
    );
    if (fixableGaps.length === 0) {
      toast.info(t("autoFixAllEmpty"));
      return;
    }
    const combinedRequest = fixableGaps
      .map((g) => gapToEditRequest(g, tGap))
      .join("\n");
    setCommandBarInput(combinedRequest);
  }, [setCommandBarInput, report.gaps, t, tGap]);

  // Count by severity
  const counts = useMemo(() => {
    const c: Record<QualityGapSeverity, number> = { high: 0, medium: 0, low: 0 };
    for (const gap of report.gaps) c[gap.severity]++;
    return c;
  }, [report.gaps]);

  // Count AI-fixable gaps for the Auto-fix All button
  const aiFixableCount = useMemo(
    () => report.gaps.filter(
      (g) => getGapEntityId(g) !== null && AI_FIXABLE_CATEGORIES.has(g.category),
    ).length,
    [report.gaps],
  );

  // Filter + sort by severity, then group by category. The text filter
  // matches against the localized issue + suggestion (so the user types
  // what they see) plus the location string for entity-name lookups.
  const grouped = useMemo(() => {
    const query = searchQuery.toLowerCase().trim();
    const filtered = report.gaps
      .filter((g) => enabledSeverities.has(g.severity))
      .filter((g) => {
        if (!query) return true;
        const issue = localizeQualityGapIssue(g, tGap).toLowerCase();
        const suggestion = localizeQualityGapSuggestion(g, tGap).toLowerCase();
        return (
          issue.includes(query) ||
          suggestion.includes(query) ||
          formatGapLocation(g.location).toLowerCase().includes(query)
        );
      })
      .sort((a, b) => SEVERITY_ORDER[a.severity] - SEVERITY_ORDER[b.severity]);

    const map = new Map<QualityGapCategory, QualityGap[]>();
    for (const gap of filtered) {
      const list = map.get(gap.category);
      if (list) list.push(gap);
      else map.set(gap.category, [gap]);
    }
    return map;
  }, [report.gaps, enabledSeverities, searchQuery, tGap]);

  const toggleSeverity = (s: QualityGapSeverity) => {
    setEnabledSeverities((prev) => {
      const next = new Set(prev);
      if (next.has(s)) next.delete(s);
      else next.add(s);
      return next;
    });
  };

  const toggleCategory = (cat: string) => {
    setCollapsedCategories((prev) => {
      const next = new Set(prev);
      if (next.has(cat)) next.delete(cat);
      else next.add(cat);
      return next;
    });
  };

  if (report.gaps.length === 0) {
    return (
      <p className="text-xs text-foreground-muted">{t("reportNoGaps")}</p>
    );
  }

  const severityLabel = (sev: QualityGapSeverity) =>
    sev === "high" ? t("highLabel") : sev === "medium" ? t("mediumLabel") : t("lowLabel");

  return (
    <div className="space-y-3">
      {/* Summary + filter toggles */}
      <div className="flex flex-wrap items-center gap-2">
        {(["high", "medium", "low"] as const).map((sev) =>
          counts[sev] > 0 ? (
            <button
              key={sev}
              type="button"
              aria-label={t("toggleAria", {
                action: enabledSeverities.has(sev) ? t("hide") : t("show"),
                severity: severityLabel(sev),
              })}
              aria-pressed={enabledSeverities.has(sev)}
              onClick={() => toggleSeverity(sev)}
              className={cn(
                "flex items-center gap-1 rounded-md border px-2 py-1 text-xs transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)]",
                enabledSeverities.has(sev)
                  ? "border-divider bg-surface-base"
                  : "border-divider bg-surface-inset opacity-40",
              )}
            >
              <span className={countBadgeClass(sev)}>{counts[sev]}</span>
              <span className="capitalize">{severityLabel(sev)}</span>
            </button>
          ) : null,
        )}

        {activeOntologyDraft && aiFixableCount > 0 && (
          <button
            type="button"
            onClick={fixAll}
            className={cn(
              "ms-auto flex items-center gap-1 rounded-md border px-2 py-1 text-xs font-medium transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)]",
              "border-concept-border bg-concept-surface text-concept-foreground hover:bg-concept-surface",
            )}
          >
            <Wand2 className="h-3 w-3" />
            {t("autoFixAll")}
          </button>
        )}
      </div>

      {/* Search */}
      <FormInput
        type="text"
        value={searchQuery}
        onChange={(e) => setSearchQuery(e.target.value)}
        placeholder={t("searchPlaceholder")}
        density="settings"
      />

      {/* Grouped gaps */}
      {grouped.size === 0 && (
        <p className="text-xs text-foreground-muted">{t("noMatches")}</p>
      )}

      {Array.from(grouped.entries()).map(([category, gaps]) => {
        const collapsed = collapsedCategories.has(category);
        return (
          <div key={category}>
            <button
              type="button"
              onClick={() => toggleCategory(category)}
              className="flex w-full items-center gap-1.5 py-1 text-start text-2xs font-semibold uppercase tracking-wider text-foreground-muted hover:text-foreground-muted"
            >
              <span
                className={cn(
                  "transition-transform duration-[var(--duration-quick)] ease-[var(--ease-out)] text-2xs",
                  collapsed ? "rotate-0" : "rotate-90",
                )}
              >
                ▶
              </span>
              {formatCategory(category, t)}
              <span className="text-foreground-muted">{t("groupCount", { count: gaps.length })}</span>
            </button>

            {!collapsed && (
              <div className="mt-1 space-y-1.5">
                {gaps.map((gap, i) => {
                  const gapIndex = report.gaps.indexOf(gap);
                  return (
                    <QualityGapCard
                      key={`${category}-${i}`}
                      gap={gap}
                      gapIndex={gapIndex}
                      isAcknowledging={acknowledgingIndex === gapIndex}
                      hasActiveOntologyDraft={!!activeOntologyDraft}
                      onFix={fixGap}
                      onAcknowledge={acknowledgeGap}
                      onNavigateToClarification={navigateToClarification}
                    />
                  );
                })}
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}
