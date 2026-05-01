"use client";

import { useTranslations } from "next-intl";
import { HugeiconsIcon } from "@hugeicons/react";
import { Database01Icon } from "@hugeicons/core-free-icons";

import { Tooltip } from "@/components/ui/tooltip";
import { useAppStore } from "@/lib/store";

/**
 * Compact chip surfacing the project's `AnalysisScope` — the union
 * of every table the project has modeled (`included`) and the
 * tables the operator has acknowledged but skipped (`deferred`).
 * Rendered in the design canvas top bar so the operator can answer
 * "where am I in this project's coverage?" without leaving design.
 *
 * Returns `null` when there is no active project or when the scope
 * is empty (BaseOntology / CodeRepository origins, or pre-analyse
 * state) — nothing meaningful to show until the first
 * introspection lands a table list.
 */
export function ScopeBadge() {
  const t = useTranslations("workbench.design.scope");
  const project = useAppStore((s) => s.activeProject);
  const scope = project?.analysis_scope;
  if (!scope) return null;

  const included = scope.included ?? [];
  const deferred = scope.deferred ?? [];
  if (included.length === 0 && deferred.length === 0) return null;

  const summary = t("summary", {
    modeled: included.length,
    deferred: deferred.length,
  });

  return (
    <Tooltip
      content={
        <div className="max-w-xs">
          <p className="mb-1 text-[11px] font-semibold uppercase tracking-wider text-emerald-300">
            {t("tooltipTitle", {
              modeled: included.length,
              deferred: deferred.length,
            })}
          </p>
          {included.length > 0 && (
            <>
              <p className="mt-1 text-[10px] uppercase text-emerald-300">
                {t("includedLabel")}
              </p>
              <ul className="space-y-0.5 font-mono text-[10px]">
                {included.slice(0, 12).map((table) => (
                  <li key={table}>{table}</li>
                ))}
                {included.length > 12 && (
                  <li className="italic text-muted-foreground">
                    {t("more", { n: included.length - 12 })}
                  </li>
                )}
              </ul>
            </>
          )}
          {deferred.length > 0 && (
            <>
              <p className="mt-2 text-[10px] uppercase text-amber-300">
                {t("deferredLabel")}
              </p>
              <ul className="space-y-0.5 font-mono text-[10px]">
                {deferred.slice(0, 8).map((d) => (
                  <li key={d.table}>
                    {d.table} —{" "}
                    <span className="italic text-muted-foreground">
                      {d.reason}
                    </span>
                  </li>
                ))}
                {deferred.length > 8 && (
                  <li className="italic text-muted-foreground">
                    {t("more", { n: deferred.length - 8 })}
                  </li>
                )}
              </ul>
            </>
          )}
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
