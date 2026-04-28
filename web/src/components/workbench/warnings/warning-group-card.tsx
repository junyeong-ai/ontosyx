"use client";

import { useMemo, useState } from "react";
import { useTranslations } from "next-intl";
import { HugeiconsIcon } from "@hugeicons/react";
import {
  Alert02Icon,
  ArrowDown01Icon,
  ArrowUp01Icon,
  InformationCircleIcon,
  Cancel01Icon,
} from "@hugeicons/core-free-icons";

import { cn } from "@/lib/cn";
import type { AnalysisWarning, WarningClass, WarningLevel } from "@/types/projects";

/**
 * Sentry / Datadog-style grouped warning view. Coalesces every
 * `AnalysisWarning` sharing a `group_key` into a single card with
 * severity chip + occurrence count + actionable hint, and lets the
 * operator expand the card to inspect the per-row scope and the raw
 * provider error string.
 *
 * The wire shape stays language-neutral: the FE looks the user-facing
 * copy up against `warnings.class.${class}` / `warnings.hint.${class}`
 * with `warning.params` interpolated. Backend never produces prose,
 * locale switches never need a server round-trip.
 */
export function WarningGroupList({
  warnings,
  className,
}: {
  warnings: ReadonlyArray<AnalysisWarning>;
  className?: string;
}) {
  const groups = useMemo(() => groupByFingerprint(warnings), [warnings]);
  if (groups.length === 0) return null;
  return (
    <div className={cn("flex flex-col gap-2", className)}>
      {groups.map((group) => (
        <WarningGroupCard key={group.groupKey} group={group} />
      ))}
    </div>
  );
}

interface WarningGroup {
  groupKey: string;
  warningClass: WarningClass;
  level: WarningLevel;
  /** First-seen scope label — used for the card header. */
  primaryLabel: string;
  /** Most-severe param set; first warning's params win for header copy. */
  params: Record<string, string>;
  warnings: AnalysisWarning[];
}

function WarningGroupCard({ group }: { group: WarningGroup }) {
  const [open, setOpen] = useState(false);
  const t = useTranslations("workbench.bottomPanel.warningGroups");
  const tClass = useTranslations(
    "workbench.bottomPanel.warningGroups.class",
  );
  const tHint = useTranslations("workbench.bottomPanel.warningGroups.hint");

  const summary = tClass(group.warningClass, {
    ...group.params,
    target: group.primaryLabel,
  });
  const hint = safeT(tHint, group.warningClass, group.params);

  return (
    <div
      className={cn(
        "rounded-md border",
        levelToBorder(group.level),
        levelToBg(group.level),
      )}
    >
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        aria-expanded={open}
        className="flex w-full items-center gap-2 px-3 py-2 text-left"
      >
        <SeverityIcon level={group.level} />
        <div className="min-w-0 flex-1">
          <p className="truncate text-xs font-medium text-zinc-800 dark:text-zinc-100">
            {summary}
          </p>
          {!open && hint && (
            <p className="truncate text-[11px] text-muted-foreground">{hint}</p>
          )}
        </div>
        <span className="rounded-full border border-zinc-300 bg-white px-1.5 py-0.5 text-[10px] font-medium text-zinc-700 dark:border-zinc-600 dark:bg-zinc-800 dark:text-zinc-200">
          {t("affectedCount", { count: group.warnings.length })}
        </span>
        <HugeiconsIcon
          icon={open ? ArrowUp01Icon : ArrowDown01Icon}
          className="h-3 w-3 shrink-0 text-muted-foreground"
          size="100%"
        />
      </button>

      {open && (
        <div className="space-y-2 border-t border-zinc-200 px-3 pb-2 pt-2 dark:border-zinc-700">
          {hint && (
            <p className="rounded-md bg-emerald-50 px-2 py-1.5 text-[11px] text-emerald-700 dark:bg-emerald-950/30 dark:text-emerald-300">
              {hint}
            </p>
          )}
          <ul className="flex flex-col gap-1">
            {group.warnings.map((w, idx) => (
              <li key={`${w.group_key}-${idx}`} className="text-[11px]">
                <p className="font-mono text-zinc-700 dark:text-zinc-300">
                  {scopeLabel(w)}
                </p>
                {w.detail && (
                  <pre className="mt-0.5 overflow-x-auto whitespace-pre-wrap break-all rounded bg-zinc-100 px-2 py-1 text-[10px] text-zinc-600 dark:bg-zinc-800/60 dark:text-zinc-400">
                    {w.detail}
                  </pre>
                )}
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}

function groupByFingerprint(
  warnings: ReadonlyArray<AnalysisWarning>,
): WarningGroup[] {
  const order: string[] = [];
  const map = new Map<string, WarningGroup>();
  for (const w of warnings) {
    // Wire-shape contract guard: a warning without a discriminated
    // `scope` violates the AnalysisWarning contract. Dropping it
    // keeps the whole panel alive in the face of one malformed row
    // (e.g. an analysis_report blob persisted under an older schema)
    // — the boundary validator at the API client logs the
    // contract violation; here we only need to keep rendering.
    if (!w || !w.scope || typeof w.scope.kind !== "string" || !w.group_key) {
      continue;
    }
    const existing = map.get(w.group_key);
    if (existing) {
      existing.warnings.push(w);
      // Most-severe level wins the header.
      if (severityRank(w.level) > severityRank(existing.level)) {
        existing.level = w.level;
      }
      continue;
    }
    order.push(w.group_key);
    map.set(w.group_key, {
      groupKey: w.group_key,
      warningClass: w.class,
      level: w.level,
      primaryLabel: scopeLabel(w),
      params: { ...(w.params ?? {}) },
      warnings: [w],
    });
  }
  return order.map((k) => map.get(k)!);
}

function scopeLabel(w: AnalysisWarning): string {
  switch (w.scope.kind) {
    case "source":
      return "source";
    case "table":
      return w.scope.name;
    case "column":
      return `${w.scope.table}.${w.scope.column}`;
  }
}

function severityRank(level: WarningLevel): number {
  switch (level) {
    case "info":
      return 0;
    case "warning":
      return 1;
    case "error":
      return 2;
  }
}

function SeverityIcon({ level }: { level: WarningLevel }) {
  switch (level) {
    case "info":
      return (
        <HugeiconsIcon
          icon={InformationCircleIcon}
          className="h-4 w-4 shrink-0 text-blue-500"
          size="100%"
        />
      );
    case "warning":
      return (
        <HugeiconsIcon
          icon={Alert02Icon}
          className="h-4 w-4 shrink-0 text-amber-500"
          size="100%"
        />
      );
    case "error":
      return (
        <HugeiconsIcon
          icon={Cancel01Icon}
          className="h-4 w-4 shrink-0 text-red-500"
          size="100%"
        />
      );
  }
}

function levelToBorder(level: WarningLevel): string {
  switch (level) {
    case "info":
      return "border-blue-200 dark:border-blue-900";
    case "warning":
      return "border-amber-200 dark:border-amber-900";
    case "error":
      return "border-red-300 dark:border-red-900";
  }
}

function levelToBg(level: WarningLevel): string {
  switch (level) {
    case "info":
      return "bg-blue-50/40 dark:bg-blue-950/20";
    case "warning":
      return "bg-amber-50/40 dark:bg-amber-950/20";
    case "error":
      return "bg-red-50/40 dark:bg-red-950/20";
  }
}

/**
 * Translate by class id, falling back to an empty string when the
 * locale catalogue has no entry — newly-added `WarningClass` variants
 * surface in the FE before the message file catches up; we render
 * "no hint" rather than crashing.
 */
function safeT(
  t: ReturnType<typeof useTranslations>,
  key: WarningClass,
  params: Record<string, string>,
): string {
  try {
    return t(key, params as Parameters<typeof t>[1]);
  } catch {
    return "";
  }
}
