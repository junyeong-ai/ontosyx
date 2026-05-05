"use client";

import { useMemo, useState } from "react";
import { useTranslations } from "next-intl";
import { AlertOctagon, ArrowDown, ArrowUp, X } from "lucide-react";
import { Info } from "lucide-react";
import { cn } from "@/lib/cn";
import { DynamicIcon } from "@/components/ui/dynamic-icon";
import type { AnalysisWarning, WarningClass, WarningLevel } from "@/types/ontology-drafts";

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
        className="flex w-full items-center gap-2 px-3 py-2 text-start"
      >
        <SeverityIcon level={group.level} />
        <div className="min-w-0 flex-1">
          <p className="truncate text-xs font-medium text-foreground-strong">
            {summary}
          </p>
          {!open && hint && (
            <p className="truncate text-2xs text-foreground-muted">{hint}</p>
          )}
        </div>
        <span className="rounded-full border border-divider bg-surface-base px-1.5 py-0.5 text-2xs font-medium text-foreground-strong">
          {t("affectedCount", { count: group.warnings.length })}
        </span>
        <DynamicIcon as={open ? ArrowUp : ArrowDown} className="h-3 w-3 shrink-0 text-foreground-muted" />
      </button>

      {open && (
        <div className="space-y-2 border-t border-divider px-3 pb-2 pt-2">
          {hint && (
            <p className="rounded-md bg-brand-surface px-2 py-1.5 text-2xs text-brand-foreground-strong">
              {hint}
            </p>
          )}
          <ul className="flex flex-col gap-1">
            {group.warnings.map((w, idx) => (
              <li key={`${w.group_key}-${idx}`} className="text-2xs">
                <p className="font-mono text-foreground">
                  {scopeLabel(w)}
                </p>
                {w.detail && (
                  <pre className="mt-0.5 overflow-x-auto whitespace-pre-wrap break-all rounded bg-surface-inset px-2 py-1 text-2xs text-foreground-subtle">
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
    if (!w?.scope || typeof w.scope.kind !== "string" || !w.group_key) {
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
        <Info className="h-4 w-4 shrink-0 text-info-foreground" />
      );
    case "warning":
      return (
        <AlertOctagon className="h-4 w-4 shrink-0 text-warning-foreground" />
      );
    case "error":
      return (
        <X className="h-4 w-4 shrink-0 text-danger-foreground" />
      );
  }
}

function levelToBorder(level: WarningLevel): string {
  switch (level) {
    case "info":
      return "border-info-border";
    case "warning":
      return "border-warning-border";
    case "error":
      return "border-danger-border";
  }
}

function levelToBg(level: WarningLevel): string {
  switch (level) {
    case "info":
      return "bg-info-surface/40";
    case "warning":
      return "bg-warning-surface";
    case "error":
      return "bg-danger-surface";
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
