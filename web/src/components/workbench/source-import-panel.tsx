"use client";

import { useMemo } from "react";
import { useTranslations } from "next-intl";

import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { RadioCard } from "@/components/ui/radio";
import { Spinner } from "@/components/ui/spinner";
import { useSourcePreview } from "@/hooks/use-source-preview";
import type { AnalyzeSelection, ProjectSource } from "@/types/projects";

type SourceImportMode = "all" | "subset" | "staged";

export interface SourceImportValue {
  mode: SourceImportMode;
  /** Always carried even in `all` mode so the user's pick survives a mode toggle. */
  selectedTables: string[];
}

export function emptyImportValue(): SourceImportValue {
  return { mode: "all", selectedTables: [] };
}

/**
 * Lower the panel value to wire-shape `AnalyzeSelection`. Extend
 * intent always emits `{ kind: "extend" }`; create intent preserves
 * the picker mode.
 */
export function toAnalyzeSelection(
  value: SourceImportValue,
  intent: "create" | "extend",
): AnalyzeSelection {
  if (value.mode === "all") return { kind: "all" };
  if (intent === "extend") {
    return { kind: "extend", tables: value.selectedTables };
  }
  if (value.mode === "staged") {
    return { kind: "staged", tables: value.selectedTables };
  }
  return { kind: "subset", tables: value.selectedTables };
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
      <fieldset
        className="grid grid-cols-3 gap-2"
        aria-label={t("modeLabel")}
      >
        {(["all", "subset", "staged"] as const).map((m) => (
          <RadioCard
            key={m}
            name="source-import-mode"
            value={m}
            checked={value.mode === m}
            onChange={() => setMode(m)}
            title={t(`modes.${m}.label`)}
            hint={t(`modes.${m}.hint`)}
          />
        ))}
      </fieldset>

      {(value.mode === "subset" || value.mode === "staged") && (
        <div className="flex flex-col gap-2">
          <div className="flex items-center justify-between gap-2">
            <p className="text-xs font-medium text-foreground">
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
            <p className="rounded border border-danger-border bg-danger-surface p-3 text-xs text-danger-foreground">
              {t("error", { error })}
            </p>
          )}

          {tables && tables.length === 0 && (
            <p className="rounded border border-warning-border bg-warning-surface p-3 text-xs text-warning-foreground">
              {t("emptyTables")}
            </p>
          )}

          {tables && tables.length > 0 && (
            <ul className="max-h-96 overflow-y-auto rounded border border-divider bg-surface-base">
              {tables.map((row) => (
                <li
                  key={row.name}
                  className="border-b border-divider-soft last:border-b-0"
                >
                  <label className="flex cursor-pointer items-center gap-3 px-3 py-2 hover:bg-surface-raised">
                    <Checkbox
                      checked={selectedSet.has(row.name)}
                      onChange={() => toggleTable(row.name)}
                    />
                    <span className="font-mono text-xs text-foreground-strong">
                      {row.name}
                    </span>
                    <span className="ms-auto flex items-center gap-2 text-2xs text-foreground-subtle">
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
