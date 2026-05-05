"use client";

import { useMemo, useState } from "react";
import { useRouter } from "next/navigation";
import { useTranslations } from "next-intl";
import { cn } from "@/lib/cn";
import { useAppStore } from "@/lib/store";
import { Tooltip } from "@/components/ui/tooltip";
import type { QueryResult, WidgetSpec } from "@/types/api";
import { formatValue } from "./chart-utils";
import { compareKorean } from "@/lib/locale/sort";
import { useLocaleChain } from "@/hooks/use-locale-chain";

/** Maximum rows rendered in the table to prevent DOM overload */
const MAX_VISIBLE_ROWS = 200;

interface TableWidgetProps {
  spec: WidgetSpec;
  data: QueryResult;
}

type SortDir = "ASC" | "DESC";

export function TableWidget({ spec, data }: TableWidgetProps) {
  const t = useTranslations("widget.table");
  const localeChain = useLocaleChain();
  const [sortCol, setSortCol] = useState<string | null>(null);
  const [sortDir, setSortDir] = useState<SortDir>("ASC");
  const router = useRouter();

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
        // Korean-aware comparison: table cells commonly contain Hangul
        // labels, so use the ko-KR collator rather than native
        // localeCompare (which varies by engine/host locale).
        cmp = compareKorean(String(av), String(bv));
      }
      return sortDir === "ASC" ? cmp : -cmp;
    });
  }, [visibleRows, sortCol, sortDir]);

  return (
    <div className="space-y-1.5">
      {spec.title && (
        <h4 className="text-xs font-semibold text-foreground">
          {spec.title}
        </h4>
      )}
      <div className="max-h-80 overflow-auto rounded-lg border border-divider bg-surface-base">
        <table className="w-full text-start text-xs">
          <thead className="sticky top-0 bg-surface-raised">
            <tr>
              {columns.map(({ key, label }) => (
                <th
                  key={key}
                  scope="col"
                  aria-sort={
                    sortCol === key
                      ? sortDir === "ASC"
                        ? "ascending"
                        : "descending"
                      : "none"
                  }
                  onClick={() => handleSort(key)}
                  className={cn(
                    "cursor-pointer select-none whitespace-nowrap px-3 py-2 font-semibold",
                    "text-foreground",
                    "hover:bg-surface-inset",
                    "transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)]",
                    sortCol === key &&
                      "text-brand-foreground",
                  )}
                >
                  {label}
                  {sortCol === key && (
                    <span className="ms-1 text-2xs">
                      {sortDir === "ASC" ? "\u2191" : "\u2193"}
                    </span>
                  )}
                </th>
              ))}
            </tr>
          </thead>
          <tbody className="divide-y divide-divider-soft/50">
            {sortedRows.map((row, ri) => (
              <tr
                key={`row-${ri}`}
                onClick={() => {
                  const firstCol = columns[0];
                  if (!firstCol) return;
                  const val = row[firstCol.key];
                  useAppStore.getState().setCommandBarInput(
                    t("detailPrompt", { column: firstCol.key, value: String(val ?? "") }),
                  );
                  router.push("/analyze");
                }}
                className={cn(
                  "cursor-pointer transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] hover:bg-brand-surface",
                  ri % 2 === 1 && "bg-surface-raised/20",
                )}
              >
                {columns.map(({ key }) => {
                  const formatted = formatValue(row[key], localeChain);
                  const isTruncatable = formatted.length > 60;
                  return (
                    <td
                      key={key}
                      className="max-w-[280px] truncate whitespace-nowrap px-3 py-1.5 text-foreground"
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
      <p className="text-2xs text-foreground-muted">
        {t("rowsAndColumns", { rows: data.rows.length, columns: columns.length })}
        {isTruncated && (
          <span className="ms-1 text-warning-foreground">
            {t("showingFirst", { count: MAX_VISIBLE_ROWS })}
          </span>
        )}
      </p>
    </div>
  );
}
