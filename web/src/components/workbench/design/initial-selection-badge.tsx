"use client";

import { useTranslations } from "next-intl";
import { HugeiconsIcon } from "@hugeicons/react";
import { Database01Icon } from "@hugeicons/core-free-icons";

import { Tooltip } from "@/components/ui/tooltip";
import { useAppStore } from "@/lib/store";

/**
 * Compact chip surfacing the `AnalyzeSelection` the operator picked at
 * project creation. Rendered in the design canvas top bar so the
 * operator can answer "which tables did I bring into this project?"
 * without re-opening the bootstrap wizard.
 *
 * Returns `null` when there is no active project, when the project
 * pre-dates the persistence column (`initial_selection === null`), or
 * when the selection is `{ kind: "all" }` — for full-warehouse sweeps
 * the table list is implicit and listing it would be noise.
 */
export function InitialSelectionBadge() {
  const t = useTranslations("workbench.design.initialSelection");
  const project = useAppStore((s) => s.activeProject);
  const selection = project?.initial_selection ?? null;
  if (!selection || selection.kind === "all") return null;

  const tables = selection.tables;
  const summary = t(`kindSummary.${selection.kind}`, { count: tables.length });

  return (
    <Tooltip
      content={
        <div className="max-w-xs">
          <p className="mb-1 text-[11px] font-semibold uppercase tracking-wider text-emerald-300">
            {t(`tooltipTitle.${selection.kind}`, { count: tables.length })}
          </p>
          <ul className="space-y-0.5 font-mono text-[10px]">
            {tables.slice(0, 12).map((table) => (
              <li key={table}>{table}</li>
            ))}
            {tables.length > 12 && (
              <li className="italic text-muted-foreground">
                {t("more", { n: tables.length - 12 })}
              </li>
            )}
          </ul>
        </div>
      }
    >
      <span className="inline-flex items-center gap-1 rounded-md border border-emerald-200 bg-white px-2 py-1 text-[10px] text-emerald-700 shadow-sm dark:border-emerald-900 dark:bg-zinc-900 dark:text-emerald-300">
        <HugeiconsIcon icon={Database01Icon} className="h-3 w-3" size="100%" />
        {summary}
      </span>
    </Tooltip>
  );
}
