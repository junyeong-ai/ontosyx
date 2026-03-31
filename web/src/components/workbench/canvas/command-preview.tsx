"use client";

import { useCallback, useMemo, useRef, useState } from "react";
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
  // Flatten batch commands for individual selection
  const flatCommands = useMemo(() => flattenCommands(commands), [commands]);

  // Guard: no structural changes
  if (flatCommands.length === 0) {
    return (
      <div
        className={cn(
          "w-full rounded-xl border bg-white/95 shadow-2xl backdrop-blur-sm",
          "dark:border-zinc-700 dark:bg-zinc-900/95",
          "border-zinc-200",
        )}
      >
        <div className="px-4 py-4">
          <p className="text-sm font-medium text-zinc-700 dark:text-zinc-300">
            No structural changes needed.
          </p>
          {explanation && (
            <p className="mt-1.5 text-xs leading-relaxed text-zinc-500">
              {explanation}
            </p>
          )}
        </div>
        <div className="flex justify-end border-t border-zinc-100 px-4 py-2.5 dark:border-zinc-800">
          <button
            onClick={onCancel}
            className="flex items-center gap-1 rounded-lg px-3 py-1.5 text-xs font-medium text-zinc-500 hover:bg-zinc-100 dark:hover:bg-zinc-800"
          >
            <HugeiconsIcon icon={Cancel01Icon} className="h-3 w-3" size="100%" />
            Close
          </button>
        </div>
      </div>
    );
  }

  const prevRef = useRef(flatCommands);
  const [checked, setChecked] = useState<boolean[]>(
    () => flatCommands.map(() => true),
  );

  // Reset checked state when flatCommands reference changes (new commands prop)
  if (prevRef.current !== flatCommands) {
    prevRef.current = flatCommands;
    setChecked(flatCommands.map(() => true));
  }

  const allChecked = checked.every(Boolean);
  const noneChecked = checked.every((v) => !v);

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

  const selectedCount = checked.filter(Boolean).length;

  return (
    <div
      className={cn(
        "w-full rounded-xl border bg-white/95 shadow-2xl backdrop-blur-sm",
        "dark:border-zinc-700 dark:bg-zinc-900/95",
        "border-zinc-200",
      )}
    >
      {/* Explanation */}
      <div className="border-b border-zinc-100 px-4 py-3 dark:border-zinc-800">
        <p className="text-xs leading-relaxed text-zinc-600 dark:text-zinc-400">
          {explanation}
        </p>
      </div>

      {/* Quick actions */}
      <div className="flex items-center gap-2 border-b border-zinc-100 px-4 py-2 dark:border-zinc-800">
        <HugeiconsIcon
          icon={CheckListIcon}
          className="h-3.5 w-3.5 text-zinc-400"
          size="100%"
        />
        <span className="text-[10px] font-medium uppercase tracking-wide text-zinc-400">
          {selectedCount}/{flatCommands.length} selected
        </span>
        <div className="ml-auto flex gap-1">
          <button
            onClick={() => toggleAll(true)}
            disabled={allChecked}
            className="rounded px-2 py-0.5 text-[10px] font-medium text-emerald-600 hover:bg-emerald-50 disabled:opacity-30 dark:text-emerald-400 dark:hover:bg-emerald-950"
          >
            Select All
          </button>
          <button
            onClick={() => toggleAll(false)}
            disabled={noneChecked}
            className="rounded px-2 py-0.5 text-[10px] font-medium text-zinc-500 hover:bg-zinc-100 disabled:opacity-30 dark:hover:bg-zinc-800"
          >
            Deselect All
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
                "flex cursor-pointer items-center gap-2.5 rounded-lg px-2 py-1.5 transition-colors",
                checked[i]
                  ? "hover:bg-zinc-50 dark:hover:bg-zinc-800/50"
                  : "opacity-50 hover:bg-zinc-50 hover:opacity-70 dark:hover:bg-zinc-800/50",
              )}
            >
              <input
                type="checkbox"
                checked={checked[i]}
                onChange={() => toggle(i)}
                className="h-3.5 w-3.5 shrink-0 cursor-pointer rounded border-zinc-300 text-emerald-600 focus:ring-emerald-500 dark:border-zinc-600"
              />
              <span
                className={cn(
                  "shrink-0 rounded px-1.5 py-0.5 text-[9px] font-bold uppercase",
                  badge.color === "green" &&
                    "bg-emerald-100 text-emerald-700 dark:bg-emerald-900 dark:text-emerald-300",
                  badge.color === "red" &&
                    "bg-red-100 text-red-700 dark:bg-red-900 dark:text-red-300",
                  badge.color === "blue" &&
                    "bg-blue-100 text-blue-700 dark:bg-blue-900 dark:text-blue-300",
                )}
              >
                {badge.label}
              </span>
              <span className="min-w-0 flex-1 truncate text-xs text-zinc-700 dark:text-zinc-300">
                {formatCommand(cmd, ontology)}
              </span>
            </label>
          );
        })}
      </div>

      {/* Actions */}
      <div className="flex items-center justify-end gap-2 border-t border-zinc-100 px-4 py-2.5 dark:border-zinc-800">
        <button
          onClick={onCancel}
          className="flex items-center gap-1 rounded-lg px-3 py-1.5 text-xs font-medium text-zinc-500 hover:bg-zinc-100 dark:hover:bg-zinc-800"
        >
          <HugeiconsIcon
            icon={Cancel01Icon}
            className="h-3 w-3"
            size="100%"
          />
          Cancel
        </button>
        <button
          onClick={handleApply}
          disabled={noneChecked}
          className="flex items-center gap-1 rounded-lg bg-emerald-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-emerald-700 disabled:opacity-50"
        >
          <HugeiconsIcon
            icon={Tick01Icon}
            className="h-3 w-3"
            size="100%"
          />
          Apply {selectedCount === flatCommands.length ? "All" : `${selectedCount}`}
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
