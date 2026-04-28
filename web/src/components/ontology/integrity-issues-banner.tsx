"use client";

import { useTranslations } from "next-intl";
import { HugeiconsIcon } from "@hugeicons/react";
import { AlertCircleIcon } from "@hugeicons/core-free-icons";

import type { DiagnosticMessage } from "@/hooks/api/use-ontology-validation";

export interface IntegrityIssuesBannerProps {
  issues: readonly DiagnosticMessage[];
  /** Optional max number of issue rows to render before collapsing
   *  the rest behind a "+N more" line. Defaults to 5 — keeps a form
   *  banner from eating the screen on a freshly-imported ontology
   *  with many dangling references. */
  maxVisible?: number;
}

/**
 * Inline warning panel listing referential-integrity diagnostics.
 *
 * Mounted by admin forms (Rule, Mapping, …) under their submit
 * button so the operator sees missing-reference warnings before
 * the save attempt would surface them via a 422. Backend stays
 * authoritative — this is a preview, not a parallel validator.
 *
 * Returns `null` for an empty list so the consumer doesn't need
 * to guard at every call site.
 */
export function IntegrityIssuesBanner({
  issues,
  maxVisible = 5,
}: IntegrityIssuesBannerProps) {
  const t = useTranslations("ontology.integrityIssues");
  if (issues.length === 0) return null;

  const visible = issues.slice(0, maxVisible);
  const hidden = issues.length - visible.length;

  return (
    <div className="rounded-md border border-amber-200 bg-amber-50 p-3 dark:border-amber-900/50 dark:bg-amber-950/30">
      <div className="mb-2 flex items-center gap-1.5">
        <HugeiconsIcon
          icon={AlertCircleIcon}
          className="h-3.5 w-3.5 text-amber-600 dark:text-amber-400"
          size="100%"
        />
        <span className="text-[11px] font-semibold uppercase tracking-wider text-amber-700 dark:text-amber-300">
          {t("heading", { count: issues.length })}
        </span>
      </div>
      <ul className="space-y-1.5">
        {visible.map((issue, index) => (
          <li key={`${issue.code}-${index}`} className="text-[11px]">
            <span className="font-mono text-amber-700 dark:text-amber-400">
              {issue.code}
            </span>
            <span className="ml-2 text-amber-800 dark:text-amber-200">
              {issue.message}
            </span>
          </li>
        ))}
        {hidden > 0 && (
          <li className="text-[10px] italic text-muted-foreground">
            {t("more", { n: hidden })}
          </li>
        )}
      </ul>
    </div>
  );
}
