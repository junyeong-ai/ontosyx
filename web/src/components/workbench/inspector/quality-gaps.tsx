"use client";

import { useState } from "react";
import { useTranslations } from "next-intl";
import { cn } from "@/lib/cn";
import { HugeiconsIcon } from "@hugeicons/react";
import { MagicWand01Icon } from "@hugeicons/core-free-icons";
import { Tooltip } from "@/components/ui/tooltip";
import { Spinner } from "@/components/ui/spinner";
import type { QualityGap } from "@/types/api";
import { getGapEntityId, navigateToGap } from "@/lib/quality-utils";
import { gapToEditRequest } from "@/lib/gap-to-edit-request";
import { localizeQualityGap } from "@/lib/quality-gap-text";
import { useAiEdit, AiSuggestionList } from "./ai-suggestions";
import { Section } from "./shared";

// ---------------------------------------------------------------------------
// Quality gaps list (with "Fix with AI" buttons)
// ---------------------------------------------------------------------------

export function QualityGapsList({
  gaps,
}: {
  gaps: QualityGap[];
}) {
  const tGap = useTranslations("qualityGap");
  const { canEdit, loading, suggestions, requestEdit, dismiss } = useAiEdit();
  const [fixingIndex, setFixingIndex] = useState<number | null>(null);

  if (gaps.length === 0) return null;

  const handleFix = async (gap: QualityGap, index: number) => {
    setFixingIndex(index);
    const request = gapToEditRequest(gap, tGap);
    await requestEdit(request);
    setFixingIndex(null);
  };

  return (
    <Section title={`Quality Issues (${gaps.length})`}>
      {suggestions && (
        <AiSuggestionList
          commands={suggestions.commands}
          explanation={suggestions.explanation}
          onDismiss={dismiss}
        />
      )}
      {gaps.map((gap, i) => {
        const focusable = getGapEntityId(gap) !== null;
        const { issue, suggestion } = localizeQualityGap(gap, tGap);
        return (
          <div
            key={i}
            className={cn(
              "px-3 py-1.5",
              focusable && "cursor-pointer hover:bg-surface-raised",
            )}
          >
            <div className="flex items-center gap-1.5">
              <Tooltip content={`Severity: ${gap.severity}`}>
                <span
                  onClick={focusable ? () => navigateToGap(gap) : undefined}
                  className={cn(
                    "h-1.5 w-1.5 shrink-0 rounded-full",
                    gap.severity === "high"
                      ? "bg-danger-solid"
                      : gap.severity === "medium"
                        ? "bg-warning-foreground"
                        : "bg-info-surface",
                  )}
                />
              </Tooltip>
              <span
                onClick={focusable ? () => navigateToGap(gap) : undefined}
                className="min-w-0 flex-1 truncate text-foreground dark:text-muted-foreground"
              >
                {issue}
              </span>
              {canEdit && (
                <Tooltip content="Fix with AI">
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      handleFix(gap, i);
                    }}
                    disabled={loading}
                    aria-label="Fix with AI"
                    className="shrink-0 rounded p-0.5 text-concept-foreground opacity-40 transition-opacity hover:bg-concept-surface hover:opacity-100 hover:text-concept-foreground dark:hover:bg-concept-surface"
                  >
                    {fixingIndex === i && loading ? (
                      <Spinner size="xs" />
                    ) : (
                      <HugeiconsIcon icon={MagicWand01Icon} className="h-2.5 w-2.5" size="100%" />
                    )}
                  </button>
                </Tooltip>
              )}
            </div>
            <p
              onClick={focusable ? () => navigateToGap(gap) : undefined}
              className="mt-0.5 truncate pl-3 text-muted-foreground"
            >
              <Tooltip content={suggestion}>
                <span className="cursor-default">{suggestion}</span>
              </Tooltip>
            </p>
          </div>
        );
      })}
    </Section>
  );
}
