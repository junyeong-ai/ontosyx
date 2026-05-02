"use client";

import { type ReactNode } from "react";
import { cn } from "@/lib/cn";

export interface DataTableColumn<T> {
  /** Stable key — used for React row keys when no `getRowId` is supplied. */
  key: string;
  header: ReactNode;
  /** Cell renderer. */
  cell: (row: T, index: number) => ReactNode;
  /** Tailwind classes added to the cell `<td>` and the matching `<th>`. */
  align?: "left" | "right" | "center";
  /** Min-width for the column — kept on both header and cell so resizable
   *  containers don't crush the column below readability. */
  minWidth?: number;
  /** Skip header rendering for this column (e.g. action menu columns). */
  hideHeader?: boolean;
}

interface DataTableProps<T> {
  columns: DataTableColumn<T>[];
  rows: T[];
  /** Stable identifier for a row. Defaults to array index. */
  getRowId?: (row: T, index: number) => string;
  /** Optional row click handler — turns rows into hover-able buttons. */
  onRowClick?: (row: T) => void;
  /** Empty-state node rendered when `rows` is empty. */
  empty?: ReactNode;
  /** Sticky-header sentinel. Use inside scrollable parents. */
  sticky?: boolean;
  /** Outer wrapper className — typically a Card. */
  className?: string;
}

const alignClass = {
  left: "text-left",
  right: "text-right",
  center: "text-center",
};

export function DataTable<T>({
  columns,
  rows,
  getRowId,
  onRowClick,
  empty,
  sticky = false,
  className,
}: DataTableProps<T>) {
  return (
    <div className={cn("overflow-auto rounded-lg border border-divider", className)}>
      <table className="w-full text-sm">
        <thead
          className={cn(
            "border-b border-divider bg-surface-raised text-2xs font-semibold uppercase tracking-wider text-foreground-muted",
            sticky && "sticky top-0 z-10",
          )}
        >
          <tr>
            {columns.map((col) => (
              <th
                key={col.key}
                scope="col"
                className={cn(
                  "px-4 py-3",
                  alignClass[col.align ?? "left"],
                )}
                style={col.minWidth ? { minWidth: col.minWidth } : undefined}
              >
                {col.hideHeader ? <span className="sr-only">{col.header}</span> : col.header}
              </th>
            ))}
          </tr>
        </thead>
        <tbody className="divide-y divide-divider-soft">
          {rows.length === 0 ? (
            <tr>
              <td colSpan={columns.length} className="p-0">
                {empty ?? (
                  <div className="px-4 py-12 text-center text-sm text-foreground-muted">
                    데이터 없음
                  </div>
                )}
              </td>
            </tr>
          ) : (
            rows.map((row, i) => {
              const id = getRowId ? getRowId(row, i) : String(i);
              return (
                <tr
                  key={id}
                  className={cn(
                    onRowClick &&
                      "cursor-pointer transition-colors duration-[var(--duration-quick)] hover:bg-surface-raised",
                  )}
                  onClick={onRowClick ? () => onRowClick(row) : undefined}
                >
                  {columns.map((col) => (
                    <td
                      key={col.key}
                      className={cn("px-4 py-3", alignClass[col.align ?? "left"])}
                      style={col.minWidth ? { minWidth: col.minWidth } : undefined}
                    >
                      {col.cell(row, i)}
                    </td>
                  ))}
                </tr>
              );
            })
          )}
        </tbody>
      </table>
    </div>
  );
}
