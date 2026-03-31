"use client";

import { useMemo, useState } from "react";
import { cn } from "@/lib/cn";
import { useAppStore } from "@/lib/store";
import { Tooltip } from "@/components/ui/tooltip";
import type { QueryResult, WidgetSpec } from "@/types/api";
import { formatValue } from "./chart-utils";

/** Maximum rows rendered in the table to prevent DOM overload */
const MAX_VISIBLE_ROWS = 200;

interface TableWidgetProps {
  spec: WidgetSpec;
  data: QueryResult;
}

type SortDir = "ASC" | "DESC";

export function TableWidget({ spec, data }: TableWidgetProps) {
  const [sortCol, setSortCol] = useState<string | null>(null);
  const [sortDir, setSortDir] = useState<SortDir>("ASC");

  // Use column defs from spec if available, otherwise fall back to data.columns
  const columns = useMemo(() => {
    if (spec.columns && Array.isArray(spec.columns)) {
      return spec.columns.map((c: { key: string; label?: string }) => ({
        key: c.key,
        label: c.label ?? c.key,
      }));
    }
    return data.columns.map((c) => ({ key: c, label: c }));
  }, [spec.columns, data.columns]);

  const handleSort = (col: string) => {
    if (sortCol === col) {
      setSortDir((d) => (d === "ASC" ? "DESC" : "ASC"));
    } else {
      setSortCol(col);
      setSortDir("ASC");
    }
  };

  const visibleRows = useMemo(
    () => data.rows.length > MAX_VISIBLE_ROWS ? data.rows.slice(0, MAX_VISIBLE_ROWS) : data.rows,
    [data.rows],
  );

  const isTruncated = data.rows.length > MAX_VISIBLE_ROWS;

  const sortedRows = useMemo(() => {
    if (!sortCol) return visibleRows;
    return [...visibleRows].sort((a, b) => {
      const av = a[sortCol];
      const bv = b[sortCol];
      if (av == null && bv == null) return 0;
      if (av == null) return 1;
      if (bv == null) return -1;

      let cmp: number;
      if (typeof av === "number" && typeof bv === "number") {
        cmp = av - bv;
      } else {
        cmp = String(av).localeCompare(String(bv));
      }
      return sortDir === "ASC" ? cmp : -cmp;
    });
  }, [visibleRows, sortCol, sortDir]);

  return (
    <div className="space-y-1.5">
      {spec.title && (
        <h4 className="text-xs font-semibold text-zinc-600 dark:text-zinc-400">
          {spec.title}
        </h4>
      )}
      <div className="max-h-80 overflow-auto rounded-lg border border-zinc-200 bg-white dark:border-zinc-700 dark:bg-zinc-800/50">
        <table className="w-full text-left text-xs">
          <thead className="sticky top-0 bg-zinc-50 dark:bg-zinc-800">
            <tr>
              {columns.map(({ key, label }) => (
                <th
                  key={key}
                  onClick={() => handleSort(key)}
                  className={cn(
                    "cursor-pointer select-none whitespace-nowrap px-3 py-2 font-semibold",
                    "text-zinc-600 dark:text-zinc-400",
                    "hover:bg-zinc-100 dark:hover:bg-zinc-700",
                    "transition-colors",
                    sortCol === key &&
                      "text-emerald-600 dark:text-emerald-400",
                  )}
                >
                  {label}
                  {sortCol === key && (
                    <span className="ml-1 text-[10px]">
                      {sortDir === "ASC" ? "\u2191" : "\u2193"}
                    </span>
                  )}
                </th>
              ))}
            </tr>
          </thead>
          <tbody className="divide-y divide-zinc-100 dark:divide-zinc-700/50">
            {sortedRows.map((row, ri) => (
              <tr
                key={`row-${ri}`}
                onClick={() => {
                  const firstCol = columns[0];
                  if (!firstCol) return;
                  const val = row[firstCol.key];
                  const store = useAppStore.getState();
                  store.setWorkspaceMode("analyze");
                  store.setCommandBarInput(`Show details where ${firstCol.key} = '${String(val ?? "")}'`);
                }}
                className={cn(
                  "cursor-pointer transition-colors hover:bg-emerald-50 dark:hover:bg-emerald-950/20",
                  ri % 2 === 1 && "bg-zinc-50/50 dark:bg-zinc-800/20",
                )}
              >
                {columns.map(({ key }) => {
                  const formatted = formatValue(row[key]);
                  const isTruncatable = formatted.length > 60;
                  return (
                    <td
                      key={key}
                      className="max-w-[280px] truncate whitespace-nowrap px-3 py-1.5 text-zinc-700 dark:text-zinc-300"
                    >
                      {isTruncatable ? (
                        <Tooltip content={formatted}>
                          <span className="cursor-default">{formatted}</span>
                        </Tooltip>
                      ) : (
                        formatted
                      )}
                    </td>
                  );
                })}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <p className="text-[10px] text-zinc-400">
        {data.rows.length} row{data.rows.length !== 1 ? "s" : ""} ·{" "}
        {columns.length} column{columns.length !== 1 ? "s" : ""}
        {isTruncated && (
          <span className="ml-1 text-amber-500">
            · Showing first {MAX_VISIBLE_ROWS} rows
          </span>
        )}
      </p>
    </div>
  );
}
