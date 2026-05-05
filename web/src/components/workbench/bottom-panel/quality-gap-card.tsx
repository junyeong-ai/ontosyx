"use client";

import { useTranslations } from "next-intl";
import { cn } from "@/lib/cn";
import { HugeiconsIcon } from "@hugeicons/react";
import { MagicWand01Icon, Tick01Icon, Alert01Icon } from "@hugeicons/core-free-icons";
import type { QualityGap, QualityGapSeverity, QualityGapCategory } from "@/types/api";
import { formatGapLocation } from "./design-panel-shared";
import { localizeQualityGap } from "@/lib/quality-gap-text";
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

function actionHintKey(actionType: GapActionType): string | null {
  switch (actionType) {
    case "ai_fix":
      return "actionAiFix";
    case "user_decision":
      return "actionUserDecision";
    case "clarification_needed":
      return "actionClarificationNeeded";
    case "info":
      return null;
  }
}

// Re-export so the parent can use it for fixAll logic
export { AI_FIXABLE_CATEGORIES };

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function severityBadgeClass(severity: QualityGapSeverity): string {
  return cn(
    "rounded px-1 py-0.5 text-2xs font-medium uppercase",
    severity === "high"
      ? "bg-danger-surface text-danger-foreground"
      : severity === "medium"
        ? "bg-warning-surface text-warning-foreground"
        : "bg-surface-inset text-foreground",
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
  const t = useTranslations("workbench.bottomPanel.quality");
  const tGap = useTranslations("qualityGap");
  const focusable = getGapEntityId(gap) !== null;
  const actionType = getGapActionType(gap.category);
  const hintKey = actionHintKey(actionType);
  const actionHint = hintKey ? t(hintKey as "actionAiFix" | "actionUserDecision" | "actionClarificationNeeded") : "";
  const { issue, suggestion } = localizeQualityGap(gap, tGap);

  return (
    <div
      className={cn(
        "rounded border border-divider bg-surface-base p-2",
        focusable &&
          "cursor-pointer hover:bg-surface-inset",
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
        <span className="text-2xs text-foreground-muted">
          {formatGapLocation(gap.location)}
        </span>
        <span className="ms-auto flex items-center gap-1.5">
          {hasActiveProject && actionType === "ai_fix" && (
            <button
              type="button"

              onClick={(e) => {
                e.stopPropagation();
                onFix(gap);
              }}
              className={cn(
                "flex items-center gap-0.5 rounded px-1.5 py-0.5 text-2xs font-medium transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)]",
                "bg-concept-surface text-concept-foreground hover:bg-concept-surface",
                "disabled:opacity-50 disabled:cursor-not-allowed",
              )}
            >
              <HugeiconsIcon icon={MagicWand01Icon} className="h-2.5 w-2.5" size="100%" />
              {t("categoryFix")}
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
                "flex items-center gap-0.5 rounded px-1.5 py-0.5 text-2xs font-medium transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)]",
                "bg-concept-surface text-concept-foreground hover:bg-concept-surface",
                "disabled:opacity-50 disabled:cursor-not-allowed",
              )}
            >
              <HugeiconsIcon icon={MagicWand01Icon} className="h-2.5 w-2.5" size="100%" />
              {t("categoryAskAi")}
            </button>
            <button
              type="button"
              disabled={isAcknowledging}
              onClick={(e) => {
                e.stopPropagation();
                onAcknowledge(gap, gapIndex);
              }}
              className={cn(
                "flex items-center gap-0.5 rounded px-1.5 py-0.5 text-2xs font-medium transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)]",
                "bg-brand-surface-strong text-brand-foreground hover:bg-brand-surface-strong",
                "disabled:opacity-50 disabled:cursor-not-allowed",
              )}
            >
              <HugeiconsIcon icon={Tick01Icon} className="h-2.5 w-2.5" size="100%" />
              {isAcknowledging ? t("categorySaving") : t("categoryConfirm")}
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
                "flex items-center gap-0.5 rounded px-1.5 py-0.5 text-2xs font-medium transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)]",
                "bg-warning-surface text-warning-foreground hover:bg-warning-surface",
              )}
            >
              <HugeiconsIcon icon={Alert01Icon} className="h-2.5 w-2.5" size="100%" />
              {t("categoryAddClarification")}
            </button>
          )}
          {focusable && (
            <span className="text-2xs text-foreground-muted">
              {t("navigate")}
            </span>
          )}
        </span>
      </div>
      <p className="mt-1 text-xs text-foreground-strong">
        {issue}
      </p>
      <p className="mt-0.5 text-2xs text-foreground-muted">
        {suggestion}
      </p>
      {actionHint && (
        <p className={cn(
          "mt-1 text-2xs font-medium",
          actionType === "ai_fix"
            ? "text-concept-foreground"
            : actionType === "user_decision"
              ? "text-foreground-muted"
              : "text-warning-foreground",
        )}>
          {actionHint}
        </p>
      )}
    </div>
  );
}
