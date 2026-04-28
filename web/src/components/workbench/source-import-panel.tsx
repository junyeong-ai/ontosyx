"use client";

// ---------------------------------------------------------------------------
// SourceImportPanel — pick a `ProjectSource` analysis mode (all vs.
// subset) plus, in subset mode, the actual table list.
//
// Reused by:
// - bootstrap step 2b (initial scoping during workspace setup)
// - Design-mode "Import Tables" action (post-create incremental
//   growth; same component, different surrounding shell)
//
// The panel is **controlled** — callers own the `value` state so
// the surrounding UI (wizard step / modal) can persist or reset
// the selection on its own schedule.
// ---------------------------------------------------------------------------

import { useMemo } from "react";
import { useTranslations } from "next-intl";

import { Button } from "@/components/ui/button";
import { Spinner } from "@/components/ui/spinner";
import { useSourcePreview } from "@/hooks/use-source-preview";
import type { AnalyzeSelection, ProjectSource } from "@/types/projects";

export type SourceImportMode = "all" | "subset";

export interface SourceImportValue {
  mode: SourceImportMode;
  /** Always carried even in `all` mode so the user's pick survives a mode toggle. */
  selectedTables: string[];
}

export function emptyImportValue(): SourceImportValue {
  return { mode: "all", selectedTables: [] };
}

/**
 * Lower the panel's local state to the wire-shape `AnalyzeSelection`
 * the backend expects. `extend` is decided by the surrounding flow
 * (Design-mode import maps `subset` → `extend`; bootstrap maps
 * `subset` → `subset`).
 */
export function toAnalyzeSelection(
  value: SourceImportValue,
  intent: "create" | "extend",
): AnalyzeSelection {
  if (value.mode === "all") return { kind: "all" };
  return intent === "extend"
    ? { kind: "extend", tables: value.selectedTables }
    : { kind: "subset", tables: value.selectedTables };
}

interface Props {
  source: ProjectSource | null;
  value: SourceImportValue;
  onChange: (next: SourceImportValue) => void;
}

export function SourceImportPanel({ source, value, onChange }: Props) {
  const t = useTranslations("source-import");

  const previewQuery = useSourcePreview(source);
  const tables = previewQuery.data?.tables ?? null;
  const loading = previewQuery.isLoading;
  const error = previewQuery.error
    ? previewQuery.error.message
    : null;

  const selectedSet = useMemo(
    () => new Set(value.selectedTables),
    [value.selectedTables],
  );

  const setMode = (mode: SourceImportMode) =>
    onChange({ ...value, mode });

  const toggleTable = (name: string) => {
    const next = new Set(selectedSet);
    if (next.has(name)) next.delete(name);
    else next.add(name);
    onChange({ ...value, selectedTables: Array.from(next) });
  };

  const selectAll = () => {
    if (!tables) return;
    onChange({ ...value, selectedTables: tables.map((row) => row.name) });
  };

  const clearSelection = () =>
    onChange({ ...value, selectedTables: [] });

  return (
    <div className="flex flex-col gap-3">
      {/* Mode toggle — drives whether the table list is required. */}
      <fieldset
        className="grid grid-cols-2 gap-2"
        aria-label={t("modeLabel")}
      >
        {(["all", "subset"] as const).map((m) => (
          <label
            key={m}
            className={`cursor-pointer rounded border px-3 py-3 text-xs ${
              value.mode === m
                ? "border-violet-500 bg-violet-50 text-violet-700 dark:bg-violet-950/40 dark:text-violet-300"
                : "border-zinc-200 bg-white text-muted-foreground hover:bg-zinc-50 dark:border-zinc-700 dark:bg-zinc-900 dark:hover:bg-zinc-800"
            }`}
          >
            <input
              type="radio"
              name="source-import-mode"
              value={m}
              checked={value.mode === m}
              onChange={() => setMode(m)}
              className="sr-only"
            />
            <p className="font-medium">{t(`modes.${m}.label`)}</p>
            <p className="mt-0.5 text-[10px] text-muted-foreground">
              {t(`modes.${m}.hint`)}
            </p>
          </label>
        ))}
      </fieldset>

      {value.mode === "subset" && (
        <div className="flex flex-col gap-2">
          <div className="flex items-center justify-between gap-2">
            <p className="text-xs font-medium text-zinc-700 dark:text-zinc-300">
              {tables
                ? t("selectionSummary", {
                    selected: value.selectedTables.length,
                    total: tables.length,
                  })
                : t("selectionLoading")}
            </p>
            <div className="flex items-center gap-1.5">
              <Button
                size="xs"
                variant="ghost"
                onClick={selectAll}
                disabled={!tables}
              >
                {t("selectAll")}
              </Button>
              <Button
                size="xs"
                variant="ghost"
                onClick={clearSelection}
                disabled={value.selectedTables.length === 0}
              >
                {t("clearSelection")}
              </Button>
            </div>
          </div>

          {loading && (
            <div className="flex items-center justify-center py-6">
              <Spinner />
            </div>
          )}

          {error && (
            <p className="rounded border border-rose-200 bg-rose-50 p-3 text-xs text-rose-700 dark:border-rose-900 dark:bg-rose-950/30 dark:text-rose-300">
              {t("error", { error })}
            </p>
          )}

          {tables && tables.length === 0 && (
            <p className="rounded border border-amber-200 bg-amber-50 p-3 text-xs text-amber-700 dark:border-amber-900 dark:bg-amber-950/30 dark:text-amber-300">
              {t("emptyTables")}
            </p>
          )}

          {tables && tables.length > 0 && (
            <ul className="max-h-96 overflow-y-auto rounded border border-zinc-200 bg-white dark:border-zinc-700 dark:bg-zinc-900">
              {tables.map((row) => (
                <li
                  key={row.name}
                  className="border-b border-zinc-100 last:border-b-0 dark:border-zinc-800"
                >
                  <label className="flex cursor-pointer items-center gap-3 px-3 py-2 hover:bg-zinc-50 dark:hover:bg-zinc-800/50">
                    <input
                      type="checkbox"
                      checked={selectedSet.has(row.name)}
                      onChange={() => toggleTable(row.name)}
                      className="h-3.5 w-3.5 rounded border-zinc-300 text-emerald-600 focus:ring-emerald-500"
                    />
                    <span className="font-mono text-xs text-zinc-900 dark:text-zinc-100">
                      {row.name}
                    </span>
                    <span className="ml-auto flex items-center gap-2 text-[10px] text-zinc-500 dark:text-zinc-500">
                      <span>{t("columnCount", { count: row.column_count })}</span>
                      {row.estimated_row_count !== null && (
                        <span>
                          {t("rowCount", {
                            count: row.estimated_row_count,
                          })}
                        </span>
                      )}
                    </span>
                  </label>
                </li>
              ))}
            </ul>
          )}
        </div>
      )}
    </div>
  );
}
