"use client";

import { useState } from "react";
import { cn } from "@/lib/cn";
import { HugeiconsIcon } from "@hugeicons/react";
import { MagicWand01Icon } from "@hugeicons/core-free-icons";
import { Tooltip } from "@/components/ui/tooltip";
import { Spinner } from "@/components/ui/spinner";
import type { QualityGap } from "@/types/api";
import { getGapEntityId, navigateToGap } from "@/lib/quality-utils";
import { gapToEditRequest } from "@/lib/gap-to-edit-request";
import { useAiEdit, AiSuggestionList } from "./ai-suggestions";
import { Section } from "./shared";

// ---------------------------------------------------------------------------
// Quality gaps list (with "Fix with AI" buttons)
// ---------------------------------------------------------------------------

export function GapsList({
  gaps,
}: {
  gaps: QualityGap[];
}) {
  const { canEdit, loading, suggestions, requestEdit, dismiss } = useAiEdit();
  const [fixingIndex, setFixingIndex] = useState<number | null>(null);

  if (gaps.length === 0) return null;

  const handleFix = async (gap: QualityGap, index: number) => {
    setFixingIndex(index);
    const request = gapToEditRequest(gap);
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
        return (
          <div
            key={i}
            className={cn(
              "px-3 py-1.5",
              focusable && "cursor-pointer hover:bg-zinc-50 dark:hover:bg-zinc-900",
            )}
          >
            <div className="flex items-center gap-1.5">
              <Tooltip content={`Severity: ${gap.severity}`}>
                <span
                  onClick={focusable ? () => navigateToGap(gap) : undefined}
                  className={cn(
                    "h-1.5 w-1.5 shrink-0 rounded-full",
                    gap.severity === "high"
                      ? "bg-red-500"
                      : gap.severity === "medium"
                        ? "bg-amber-400"
                        : "bg-sky-400",
                  )}
                />
              </Tooltip>
              <span
                onClick={focusable ? () => navigateToGap(gap) : undefined}
                className="min-w-0 flex-1 truncate text-zinc-600 dark:text-zinc-400"
              >
                {gap.issue}
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
                    className="shrink-0 rounded p-0.5 text-violet-400 opacity-40 transition-opacity hover:bg-violet-50 hover:opacity-100 hover:text-violet-600 dark:hover:bg-violet-950"
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
              className="mt-0.5 truncate pl-3 text-zinc-400"
            >
              <Tooltip content={gap.suggestion}>
                <span className="cursor-default">{gap.suggestion}</span>
              </Tooltip>
            </p>
          </div>
        );
      })}
    </Section>
  );
}
