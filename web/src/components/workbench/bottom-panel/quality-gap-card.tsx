"use client";

import { cn } from "@/lib/cn";
import { HugeiconsIcon } from "@hugeicons/react";
import { MagicWand01Icon, Tick01Icon, Alert01Icon } from "@hugeicons/core-free-icons";
import type { QualityGap, QualityGapSeverity, QualityGapCategory } from "@/types/api";
import { formatGapLocation } from "./design-panel-shared";
import { getGapEntityId, navigateToGap } from "@/lib/quality-utils";

// ---------------------------------------------------------------------------
// Gap action classification
// ---------------------------------------------------------------------------

/** Categories where AI can generate a meaningful fix via editProject. */
const AI_FIXABLE_CATEGORIES = new Set<QualityGapCategory>([
  "missing_description",
  "missing_foreign_key_edge",
  "missing_containment_edge",
  "unmapped_source_column",
  "unmapped_source_table",
  "duplicate_edge",
  "orphan_node",
  "hub_node",
  "overloaded_property",
  "self_referential_edge",
  "property_type_inconsistency",
]);

/** Categories where the user needs to confirm that the data is intentional. */
const USER_DECISION_CATEGORIES = new Set<QualityGapCategory>([
  "single_value_bias",
  "sparse_property",
]);

/** Categories that should have been suppressed by column_clarifications.
 *  If they still appear, the user needs to add a clarification. */
const CLARIFICATION_NEEDED_CATEGORIES = new Set<QualityGapCategory>([
  "numeric_enum_code",
  "opaque_enum_value",
]);

type GapActionType = "ai_fix" | "user_decision" | "clarification_needed" | "info";

function getGapActionType(category: QualityGapCategory): GapActionType {
  if (AI_FIXABLE_CATEGORIES.has(category)) return "ai_fix";
  if (USER_DECISION_CATEGORIES.has(category)) return "user_decision";
  if (CLARIFICATION_NEEDED_CATEGORIES.has(category)) return "clarification_needed";
  return "info";
}

function getActionHint(actionType: GapActionType): string {
  switch (actionType) {
    case "ai_fix":
      return "AI can fix this -- click Fix to apply";
    case "user_decision":
      return "Ask AI for suggestions, or Confirm to acknowledge as expected";
    case "clarification_needed":
      return "Provide a clarification in the Analysis Review";
    case "info":
      return "";
  }
}

// Re-export so the parent can use it for fixAll logic
export { AI_FIXABLE_CATEGORIES };

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function severityBadgeClass(severity: QualityGapSeverity): string {
  return cn(
    "rounded px-1 py-0.5 text-[9px] font-medium uppercase",
    severity === "high"
      ? "bg-red-100 text-red-700 dark:bg-red-950 dark:text-red-300"
      : severity === "medium"
        ? "bg-amber-100 text-amber-700 dark:bg-amber-950 dark:text-amber-300"
        : "bg-zinc-100 text-zinc-600 dark:bg-zinc-800 dark:text-zinc-400",
  );
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export interface QualityGapCardProps {
  gap: QualityGap;
  gapIndex: number;
  isAcknowledging: boolean;
  hasActiveProject: boolean;
  onFix: (gap: QualityGap) => void;
  onAcknowledge: (gap: QualityGap, index: number) => void;
  onNavigateToClarification: (gap: QualityGap) => void;
}

export function QualityGapCard({
  gap,
  gapIndex,
  isAcknowledging,
  hasActiveProject,
  onFix,
  onAcknowledge,
  onNavigateToClarification,
}: QualityGapCardProps) {
  const focusable = getGapEntityId(gap) !== null;
  const actionType = getGapActionType(gap.category);
  const actionHint = getActionHint(actionType);

  return (
    <div
      className={cn(
        "rounded border border-zinc-200 bg-white p-2 dark:border-zinc-800 dark:bg-zinc-950/60",
        focusable &&
          "cursor-pointer hover:bg-zinc-100 dark:hover:bg-zinc-800",
      )}
      role={focusable ? "button" : undefined}
      tabIndex={focusable ? 0 : undefined}
      onClick={focusable ? () => navigateToGap(gap) : undefined}
      onKeyDown={focusable ? (e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          navigateToGap(gap);
        }
      } : undefined}
    >
      <div className="flex items-center gap-2">
        <span className={severityBadgeClass(gap.severity)}>
          {gap.severity}
        </span>
        <span className="text-[10px] text-zinc-500">
          {formatGapLocation(gap.location)}
        </span>
        <span className="ml-auto flex items-center gap-1.5">
          {hasActiveProject && actionType === "ai_fix" && (
            <button
              type="button"

              onClick={(e) => {
                e.stopPropagation();
                onFix(gap);
              }}
              className={cn(
                "flex items-center gap-0.5 rounded px-1.5 py-0.5 text-[9px] font-medium transition-colors",
                "bg-violet-100 text-violet-700 hover:bg-violet-200",
                "dark:bg-violet-950 dark:text-violet-300 dark:hover:bg-violet-900",
                "disabled:opacity-50 disabled:cursor-not-allowed",
              )}
            >
              <HugeiconsIcon icon={MagicWand01Icon} className="h-2.5 w-2.5" size="100%" />
              Fix
            </button>
          )}
          {hasActiveProject && actionType === "user_decision" && (
            <>
            <button
              type="button"

              onClick={(e) => {
                e.stopPropagation();
                onFix(gap);
              }}
              className={cn(
                "flex items-center gap-0.5 rounded px-1.5 py-0.5 text-[9px] font-medium transition-colors",
                "bg-violet-100 text-violet-700 hover:bg-violet-200",
                "dark:bg-violet-950 dark:text-violet-300 dark:hover:bg-violet-900",
                "disabled:opacity-50 disabled:cursor-not-allowed",
              )}
            >
              <HugeiconsIcon icon={MagicWand01Icon} className="h-2.5 w-2.5" size="100%" />
              Ask AI
            </button>
            <button
              type="button"
              disabled={isAcknowledging}
              onClick={(e) => {
                e.stopPropagation();
                onAcknowledge(gap, gapIndex);
              }}
              className={cn(
                "flex items-center gap-0.5 rounded px-1.5 py-0.5 text-[9px] font-medium transition-colors",
                "bg-emerald-100 text-emerald-700 hover:bg-emerald-200",
                "dark:bg-emerald-950 dark:text-emerald-300 dark:hover:bg-emerald-900",
                "disabled:opacity-50 disabled:cursor-not-allowed",
              )}
            >
              <HugeiconsIcon icon={Tick01Icon} className="h-2.5 w-2.5" size="100%" />
              {isAcknowledging ? "Saving..." : "Confirm"}
            </button>
            </>
          )}
          {hasActiveProject && actionType === "clarification_needed" && (
            <button
              type="button"
              onClick={(e) => {
                e.stopPropagation();
                onNavigateToClarification(gap);
              }}
              className={cn(
                "flex items-center gap-0.5 rounded px-1.5 py-0.5 text-[9px] font-medium transition-colors",
                "bg-amber-100 text-amber-700 hover:bg-amber-200",
                "dark:bg-amber-950 dark:text-amber-300 dark:hover:bg-amber-900",
              )}
            >
              <HugeiconsIcon icon={Alert01Icon} className="h-2.5 w-2.5" size="100%" />
              Add Clarification
            </button>
          )}
          {focusable && (
            <span className="text-[9px] text-zinc-400">
              Navigate →
            </span>
          )}
        </span>
      </div>
      <p className="mt-1 text-xs text-zinc-700 dark:text-zinc-200">
        {gap.issue}
      </p>
      <p className="mt-0.5 text-[10px] text-zinc-400">
        {gap.suggestion}
      </p>
      {actionHint && (
        <p className={cn(
          "mt-1 text-[10px] font-medium",
          actionType === "ai_fix"
            ? "text-violet-500 dark:text-violet-400"
            : actionType === "user_decision"
              ? "text-zinc-500 dark:text-zinc-400"
              : "text-amber-600 dark:text-amber-400",
        )}>
          {actionHint}
        </p>
      )}
    </div>
  );
}
