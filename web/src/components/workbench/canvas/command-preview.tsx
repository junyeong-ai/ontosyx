"use client";

import { useCallback, useMemo, useState } from "react";
import { useTranslations } from "next-intl";
import type { OntologyCommand, OntologyIR } from "@/types/api";
import { formatCommand, commandOpBadge } from "@/lib/command-format";
import { cn } from "@/lib/cn";
import { HugeiconsIcon } from "@hugeicons/react";
import {
  Tick01Icon,
  Cancel01Icon,
  CheckListIcon,
} from "@hugeicons/core-free-icons";

interface CommandPreviewProps {
  commands: OntologyCommand[];
  explanation: string;
  ontology?: OntologyIR | null;
  onApply: (accepted: OntologyCommand[]) => void;
  onCancel: () => void;
}

export function CommandPreview({
  commands,
  explanation,
  ontology,
  onApply,
  onCancel,
}: CommandPreviewProps) {
  const t = useTranslations("workbench.canvas.commandPreview");
  const tCommand = useTranslations("workbench.canvas.commandPreview.command");
  const tCommon = useTranslations("common");
  // Flatten batch commands for individual selection
  const flatCommands = useMemo(() => flattenCommands(commands), [commands]);

  // React canonical "reset-state-when-prop-changes" pattern:
  // track the previous prop identity in state, and reset `checked`
  // synchronously during render when it drifts.
  const [prevFlatCommands, setPrevFlatCommands] = useState(flatCommands);
  const [checked, setChecked] = useState<boolean[]>(() =>
    flatCommands.map(() => true),
  );
  if (prevFlatCommands !== flatCommands) {
    setPrevFlatCommands(flatCommands);
    setChecked(flatCommands.map(() => true));
  }

  const toggleAll = useCallback(
    (value: boolean) => setChecked(flatCommands.map(() => value)),
    [flatCommands],
  );

  const toggle = useCallback(
    (index: number) =>
      setChecked((prev) => prev.map((v, i) => (i === index ? !v : v))),
    [],
  );

  const handleApply = useCallback(() => {
    const accepted = flatCommands.filter((_, i) => checked[i]);
    if (accepted.length === 0) return;
    onApply(accepted);
  }, [flatCommands, checked, onApply]);

  // Guard: no structural changes — render after all hooks
  if (flatCommands.length === 0) {
    return (
      <div
        className={cn(
          "w-full rounded-xl border bg-surface-base shadow-4 backdrop-blur-sm",
          "border-divider",
        )}
      >
        <div className="px-4 py-4">
          <p className="text-sm font-medium text-foreground">
            {t("noChangesTitle")}
          </p>
          {explanation && (
            <p className="mt-1.5 text-xs leading-relaxed text-foreground-muted">
              {explanation}
            </p>
          )}
        </div>
        <div className="flex justify-end border-t border-divider-soft px-4 py-2.5">
          <button type="button"
            onClick={onCancel}
            className="flex items-center gap-1 rounded-lg px-3 py-1.5 text-xs font-medium text-foreground-muted hover:bg-surface-inset"
          >
            <HugeiconsIcon icon={Cancel01Icon} className="h-3 w-3" size="100%" />
            {tCommon("close")}
          </button>
        </div>
      </div>
    );
  }

  const allChecked = checked.every(Boolean);
  const noneChecked = checked.every((v) => !v);
  const selectedCount = checked.filter(Boolean).length;

  return (
    <div
      className={cn(
        "w-full rounded-xl border bg-surface-base shadow-4 backdrop-blur-sm",
        "border-divider",
      )}
    >
      {/* Explanation */}
      <div className="border-b border-divider-soft px-4 py-3">
        <p className="text-xs leading-relaxed text-foreground">
          {explanation}
        </p>
      </div>

      {/* Quick actions */}
      <div className="flex items-center gap-2 border-b border-divider-soft px-4 py-2">
        <HugeiconsIcon
          icon={CheckListIcon}
          className="h-3.5 w-3.5 text-foreground-muted"
          size="100%"
        />
        <span className="text-2xs font-medium uppercase tracking-wide text-foreground-muted">
          {t("selectedCount", { selected: selectedCount, total: flatCommands.length })}
        </span>
        <div className="ms-auto flex gap-1">
          <button type="button"
            onClick={() => toggleAll(true)}
            disabled={allChecked}
            className="rounded px-2 py-0.5 text-2xs font-medium text-brand-foreground hover:bg-brand-surface disabled:opacity-30"
          >
            {t("selectAll")}
          </button>
          <button type="button"
            onClick={() => toggleAll(false)}
            disabled={noneChecked}
            className="rounded px-2 py-0.5 text-2xs font-medium text-foreground-muted hover:bg-surface-inset disabled:opacity-30"
          >
            {t("deselectAll")}
          </button>
        </div>
      </div>

      {/* Command list */}
      <div className="max-h-56 overflow-y-auto px-2 py-1.5">
        {flatCommands.map((cmd, i) => {
          const badge = commandOpBadge(cmd);
          return (
            <label
              key={i}
              className={cn(
                "flex cursor-pointer items-center gap-2.5 rounded-lg px-2 py-1.5 transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)]",
                checked[i]
                  ? "hover:bg-surface-raised"
                  : "opacity-50 hover:bg-surface-raised hover:opacity-70",
              )}
            >
              <input
                type="checkbox"
                checked={checked[i]}
                onChange={() => toggle(i)}
                className="h-3.5 w-3.5 shrink-0 cursor-pointer rounded border-divider text-brand-foreground focus:ring-brand-foreground"
              />
              <span
                className={cn(
                  "shrink-0 rounded px-1.5 py-0.5 text-2xs font-bold uppercase",
                  badge.color === "green" &&
                    "bg-brand-surface-strong text-brand-foreground-strong",
                  badge.color === "red" &&
                    "bg-danger-surface text-danger-foreground",
                  badge.color === "blue" &&
                    "bg-info-surface text-info-foreground",
                )}
              >
                {badge.label}
              </span>
              <span className="min-w-0 flex-1 truncate text-xs text-foreground">
                {(() => {
                  const f = formatCommand(cmd, ontology);
                  return tCommand(f.key, f.params);
                })()}
              </span>
            </label>
          );
        })}
      </div>

      {/* Actions */}
      <div className="flex items-center justify-end gap-2 border-t border-divider-soft px-4 py-2.5">
        <button type="button"
          onClick={onCancel}
          className="flex items-center gap-1 rounded-lg px-3 py-1.5 text-xs font-medium text-foreground-muted hover:bg-surface-inset"
        >
          <HugeiconsIcon
            icon={Cancel01Icon}
            className="h-3 w-3"
            size="100%"
          />
          {tCommon("cancel")}
        </button>
        <button type="button"
          onClick={handleApply}
          disabled={noneChecked}
          className="flex items-center gap-1 rounded-lg bg-brand-solid px-3 py-1.5 text-xs font-medium text-foreground-onbrand hover:bg-brand-solid-hover disabled:opacity-50"
        >
          <HugeiconsIcon
            icon={Tick01Icon}
            className="h-3 w-3"
            size="100%"
          />
          {selectedCount === flatCommands.length ? t("applyAll") : t("apply", { count: selectedCount })}
        </button>
      </div>
    </div>
  );
}

/**
 * Flatten batch commands into individual commands for per-command selection.
 * Keeps non-batch commands as-is.
 */
function flattenCommands(commands: OntologyCommand[]): OntologyCommand[] {
  const result: OntologyCommand[] = [];
  for (const cmd of commands) {
    if (cmd.op === "batch") {
      result.push(...flattenCommands(cmd.commands));
    } else {
      result.push(cmd);
    }
  }
  return result;
}
