"use client";

import { useState, useMemo, useCallback } from "react";
import { cn } from "@/lib/cn";
import { HugeiconsIcon } from "@hugeicons/react";
import { MagicWand01Icon } from "@hugeicons/core-free-icons";
import { useConfirm } from "@/components/ui/confirm-dialog";
import { toast } from "sonner";
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
import { updateDecisions, getProject } from "@/lib/api";
import { useAppStore } from "@/lib/store";
import { QualityGapCard, AI_FIXABLE_CATEGORIES } from "./quality-gap-card";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const SEVERITY_ORDER: Record<QualityGapSeverity, number> = {
  high: 0,
  medium: 1,
  low: 2,
};

function formatCategory(category: QualityGapCategory): string {
  return category
    .split("_")
    .map((w) => w.charAt(0).toUpperCase() + w.slice(1))
    .join(" ");
}

function countBadgeClass(severity: QualityGapSeverity): string {
  return cn(
    "rounded-full px-1.5 py-0.5 text-[9px] font-medium tabular-nums",
    severity === "high"
      ? "bg-red-100 text-red-700 dark:bg-red-950 dark:text-red-300"
      : severity === "medium"
        ? "bg-amber-100 text-amber-700 dark:bg-amber-950 dark:text-amber-300"
        : "bg-zinc-100 text-zinc-600 dark:bg-zinc-800 dark:text-zinc-400",
  );
}

/**
 * Build an acknowledgment clarification hint based on gap type and location.
 */
function buildAcknowledgmentHint(gap: QualityGap): string {
  const loc = gap.location;
  if (gap.category === "single_value_bias") {
    // Extract value info from the issue text if possible
    const match = gap.issue.match(/all (?:values|rows) (?:are|=) ['"]?([^'"]+)['"]?/i)
      ?? gap.issue.match(/single value ['"]?([^'"]+)['"]?/i);
    const value = match?.[1] ?? "the observed value";
    return `Confirmed: '${value}' is the expected value for this column`;
  }
  if (gap.category === "sparse_property") {
    return "Confirmed: nullable property, keep as-is";
  }
  // Fallback
  if ("property_name" in loc) {
    return `Acknowledged: ${gap.issue}`;
  }
  return `Acknowledged: ${gap.issue}`;
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
  const [enabledSeverities, setEnabledSeverities] = useState<
    Set<QualityGapSeverity>
  >(new Set(["high", "medium", "low"]));
  const [collapsedCategories, setCollapsedCategories] = useState<Set<string>>(
    new Set(),
  );
  const [acknowledgingIndex, setAcknowledgingIndex] = useState<number | null>(null);
  const [searchQuery, setSearchQuery] = useState("");

  const activeProject = useAppStore((s) => s.activeProject);
  const setActiveProject = useAppStore((s) => s.setActiveProject);
  const setCommandBarInput = useAppStore((s) => s.setCommandBarInput);
  const setDesignBottomTab = useAppStore((s) => s.setDesignBottomTab);

  const fixGap = useCallback(
    (gap: QualityGap) => {
      setCommandBarInput(gapToEditRequest(gap));
    },
    [setCommandBarInput],
  );

  const confirmDialog = useConfirm();

  const acknowledgeGap = useCallback(
    async (gap: QualityGap, index: number) => {
      if (!activeProject) return;

      const sourceLoc = extractSourceLocation(gap);
      if (!sourceLoc) {
        toast.error("Cannot determine source column for this gap");
        return;
      }

      // Confirm with user before acknowledging
      const confirmed = await confirmDialog({
        title: "Acknowledge Finding",
        description: gap.category === "single_value_bias"
          ? `Confirm that the observed value is expected for ${sourceLoc.table}.${sourceLoc.column}? This will suppress the warning in future quality assessments.`
          : `Confirm that ${sourceLoc.table}.${sourceLoc.column} is intentionally sparse or can be kept as-is? This will suppress the warning.`,
        confirmLabel: "Confirm",
      });
      if (!confirmed) return;

      setAcknowledgingIndex(index);
      try {
        const hint = buildAcknowledgmentHint(gap);
        const existingClarifications = activeProject.design_options.column_clarifications ?? [];

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

        const updatedProject = await updateDecisions(activeProject.id, {
          design_options: {
            ...activeProject.design_options,
            column_clarifications: newClarifications,
          },
          revision: activeProject.revision,
        });
        setActiveProject(updatedProject);
        toast.success("Gap acknowledged", {
          description: `Clarification added for ${sourceLoc.table}.${sourceLoc.column}`,
        });
      } catch (err) {
        toast.error("Acknowledge failed", {
          description: err instanceof Error ? err.message : "Unknown error",
        });
        // Try to reload project in case of conflict
        try {
          const fresh = await getProject(activeProject.id);
          setActiveProject(fresh);
        } catch {
          /* ignore reload failure */
        }
      } finally {
        setAcknowledgingIndex(null);
      }
    },
    [activeProject, setActiveProject],
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
          if (summary?.textContent?.includes("Analysis Review")) {
            d.open = true;
            d.scrollIntoView({ behavior: "smooth", block: "start" });
            break;
          }
        }
      });

      toast.info(`Add a clarification for ${locationLabel}`, {
        description: "Provide context in the Analysis Review section",
      });
    },
    [setDesignBottomTab],
  );

  const fixAll = useCallback(() => {
    const fixableGaps = report.gaps.filter(
      (g) => getGapEntityId(g) !== null && AI_FIXABLE_CATEGORIES.has(g.category),
    );
    if (fixableGaps.length === 0) {
      toast.info("No auto-fixable gaps found");
      return;
    }
    const combinedRequest = fixableGaps
      .map((g) => gapToEditRequest(g))
      .join("\n");
    setCommandBarInput(combinedRequest);
  }, [setCommandBarInput, report.gaps]);

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

  // Filter + sort by severity, then group by category
  const grouped = useMemo(() => {
    const query = searchQuery.toLowerCase().trim();
    const filtered = report.gaps
      .filter((g) => enabledSeverities.has(g.severity))
      .filter((g) => !query || g.issue.toLowerCase().includes(query) || g.suggestion.toLowerCase().includes(query) || formatGapLocation(g.location).toLowerCase().includes(query))
      .sort((a, b) => SEVERITY_ORDER[a.severity] - SEVERITY_ORDER[b.severity]);

    const map = new Map<QualityGapCategory, QualityGap[]>();
    for (const gap of filtered) {
      const list = map.get(gap.category);
      if (list) list.push(gap);
      else map.set(gap.category, [gap]);
    }
    return map;
  }, [report.gaps, enabledSeverities, searchQuery]);

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
      <p className="text-xs text-zinc-400">No quality gaps found.</p>
    );
  }

  return (
    <div className="space-y-3">
      {/* Summary + filter toggles */}
      <div className="flex flex-wrap items-center gap-2">
        {(["high", "medium", "low"] as const).map((sev) =>
          counts[sev] > 0 ? (
            <button
              key={sev}
              type="button"
              aria-label={`${enabledSeverities.has(sev) ? "Hide" : "Show"} ${sev} severity gaps`}
              aria-pressed={enabledSeverities.has(sev)}
              onClick={() => toggleSeverity(sev)}
              className={cn(
                "flex items-center gap-1 rounded-md border px-2 py-1 text-xs transition-colors",
                enabledSeverities.has(sev)
                  ? "border-zinc-300 bg-white dark:border-zinc-700 dark:bg-zinc-800"
                  : "border-zinc-200 bg-zinc-100 opacity-40 dark:border-zinc-800 dark:bg-zinc-900",
              )}
            >
              <span className={countBadgeClass(sev)}>{counts[sev]}</span>
              <span className="capitalize">{sev}</span>
            </button>
          ) : null,
        )}

        {activeProject && aiFixableCount > 0 && (
          <button
            type="button"
            onClick={fixAll}
            className={cn(
              "ml-auto flex items-center gap-1 rounded-md border px-2 py-1 text-xs font-medium transition-colors",
              "border-violet-300 bg-violet-50 text-violet-700 hover:bg-violet-100",
              "dark:border-violet-700 dark:bg-violet-950 dark:text-violet-300 dark:hover:bg-violet-900",
            )}
          >
            <HugeiconsIcon icon={MagicWand01Icon} className="h-3 w-3" size="100%" />
            Auto-fix All
          </button>
        )}
      </div>

      {/* Search */}
      <input
        type="text"
        value={searchQuery}
        onChange={(e) => setSearchQuery(e.target.value)}
        placeholder="Search gaps by keyword..."
        className="w-full rounded-md border border-zinc-200 bg-white px-2.5 py-1.5 text-xs text-zinc-700 placeholder-zinc-500 focus:border-emerald-400 focus:ring-1 focus:ring-emerald-400/50 focus:outline-none dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-300"
      />

      {/* Grouped gaps */}
      {grouped.size === 0 && (
        <p className="text-xs text-zinc-400">No gaps match the current filter.</p>
      )}

      {Array.from(grouped.entries()).map(([category, gaps]) => {
        const collapsed = collapsedCategories.has(category);
        return (
          <div key={category}>
            <button
              type="button"
              onClick={() => toggleCategory(category)}
              className="flex w-full items-center gap-1.5 py-1 text-left text-[10px] font-semibold uppercase tracking-wider text-zinc-500 hover:text-zinc-700 dark:hover:text-zinc-300"
            >
              <span
                className={cn(
                  "transition-transform text-[8px]",
                  collapsed ? "rotate-0" : "rotate-90",
                )}
              >
                ▶
              </span>
              {formatCategory(category)}
              <span className="text-zinc-400">({gaps.length})</span>
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
                      hasActiveProject={!!activeProject}
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
