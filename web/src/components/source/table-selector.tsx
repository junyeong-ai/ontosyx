"use client";

import { useMemo, useState } from "react";
import { useTranslations } from "next-intl";

import { cn } from "@/lib/cn";
import type { PreviewTableSummary } from "@/types/projects";

/**
 * Pickable list of source tables. Renders a search box, a select-all
 * toggle, and one row per table with row count + column count + last
 * modified. The component is fully controlled — the parent owns the
 * selected set and decides what to do with it.
 *
 * Designed for the project-create flow: the user inspects a source's
 * advertised surface and curates the analysis target before paying
 * the introspection cost. Scales to a few hundred tables on plain
 * scroll; virtualisation is a follow-up for warehouses with 1k+
 * tables (`@tanstack/react-virtual` slot is left vacant on purpose).
 */
export interface TableSelectorProps {
  tables: ReadonlyArray<PreviewTableSummary>;
  selected: ReadonlySet<string>;
  onChange: (next: Set<string>) => void;
  /** Optional max-height (e.g. `"24rem"`); defaults to `"22rem"`. */
  maxHeight?: string;
  /** Disable interaction, e.g. while a parent mutation is in-flight. */
  disabled?: boolean;
}

export function TableSelector({
  tables,
  selected,
  onChange,
  maxHeight = "22rem",
  disabled,
}: TableSelectorProps) {
  const t = useTranslations("workbench.bottomPanel.tableSelector");
  const [query, setQuery] = useState("");

  const filtered = useMemo(() => {
    const needle = query.trim().toLowerCase();
    if (!needle) return tables;
    return tables.filter((tbl) => tbl.name.toLowerCase().includes(needle));
  }, [tables, query]);

  const allFilteredSelected =
    filtered.length > 0 && filtered.every((tbl) => selected.has(tbl.name));
  const someFilteredSelected = filtered.some((tbl) => selected.has(tbl.name));

  function toggleOne(name: string) {
    const next = new Set(selected);
    if (next.has(name)) next.delete(name);
    else next.add(name);
    onChange(next);
  }

  function toggleFilteredAll() {
    const next = new Set(selected);
    if (allFilteredSelected) {
      // All filtered tables are selected — unselect them.
      filtered.forEach((tbl) => next.delete(tbl.name));
    } else {
      // Some or none of the filtered tables are selected — select all.
      filtered.forEach((tbl) => next.add(tbl.name));
    }
    onChange(next);
  }

  function clearAll() {
    onChange(new Set());
  }

  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center gap-2">
        <input
          type="search"
          placeholder={t("searchPlaceholder")}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          disabled={disabled}
          className={cn(
            "min-w-0 flex-1 rounded-md border border-divider bg-transparent px-3 py-1.5 text-sm",
            "outline-none transition-colors",
            "focus:border-brand-foreground focus:ring-1 focus:ring-brand-foreground/50",
            "disabled:cursor-not-allowed disabled:opacity-50",
          )}
        />
        <span className="shrink-0 whitespace-nowrap text-xs text-muted-foreground">
          {t("selectedCount", {
            selected: selected.size,
            total: tables.length,
          })}
        </span>
      </div>

      <div className="flex items-center gap-3 text-xs">
        <label className="flex cursor-pointer items-center gap-1.5">
          <input
            type="checkbox"
            checked={allFilteredSelected}
            ref={(el) => {
              if (el)
                el.indeterminate =
                  !allFilteredSelected && someFilteredSelected;
            }}
            onChange={toggleFilteredAll}
            disabled={disabled || filtered.length === 0}
            className="h-3.5 w-3.5 accent-brand-foreground"
          />
          <span>
            {query.trim()
              ? t("selectAllFiltered", { count: filtered.length })
              : t("selectAll", { count: tables.length })}
          </span>
        </label>
        <button
          type="button"
          onClick={clearAll}
          disabled={disabled || selected.size === 0}
          className="text-brand-foreground underline-offset-2 hover:underline disabled:cursor-not-allowed disabled:text-foreground-subtle disabled:no-underline dark:disabled:text-foreground-muted"
        >
          {t("clearAll")}
        </button>
      </div>

      <div
        role="listbox"
        aria-multiselectable
        className={cn(
          "overflow-y-auto rounded-md border border-divider bg-surface-base text-sm",
          "dark:border-divider",
        )}
        style={{ maxHeight }}
      >
        {filtered.length === 0 ? (
          <p className="px-3 py-6 text-center text-xs text-muted-foreground">
            {tables.length === 0 ? t("emptyDataset") : t("noMatches")}
          </p>
        ) : (
          <ul className="divide-y divide-divider">
            {filtered.map((tbl) => {
              const isSelected = selected.has(tbl.name);
              return (
                <li
                  key={tbl.name}
                  role="option"
                  aria-selected={isSelected}
                  className={cn(
                    "flex items-center gap-3 px-3 py-2",
                    isSelected
                      ? "bg-brand-surface"
                      : "hover:bg-surface-raised",
                  )}
                >
                  <input
                    type="checkbox"
                    checked={isSelected}
                    onChange={() => toggleOne(tbl.name)}
                    disabled={disabled}
                    className="h-3.5 w-3.5 accent-brand-foreground"
                    aria-label={tbl.name}
                  />
                  <div className="flex min-w-0 flex-1 flex-col">
                    <span className="truncate font-mono text-xs text-foreground-strong">
                      {tbl.name}
                    </span>
                    <span className="text-xs text-muted-foreground">
                      {formatRowCount(tbl.estimated_row_count, t)}
                      {" · "}
                      {t("columnCount", { count: tbl.column_count })}
                      {tbl.last_modified
                        ? " · " +
                          t("lastModifiedAt", {
                            timestamp: formatTimestamp(tbl.last_modified),
                          })
                        : ""}
                    </span>
                  </div>
                </li>
              );
            })}
          </ul>
        )}
      </div>
    </div>
  );
}

function formatRowCount(
  count: number | null | undefined,
  t: ReturnType<typeof useTranslations>,
): string {
  if (count == null) return t("rowsUnknown");
  return t("rowCount", { count });
}

function formatTimestamp(iso: string): string {
  // Use the user's locale; falls back gracefully if the value is not
  // a parseable ISO string (we render the original string then).
  const d = new Date(iso);
  if (Number.isNaN(d.valueOf())) return iso;
  return d.toLocaleString();
}
