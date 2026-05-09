"use client";

import { type ReactNode, useEffect, useMemo } from "react";
import {
  flexRender,
  getCoreRowModel,
  getSortedRowModel,
  type ColumnDef,
  type ColumnSort,
  type Row,
  type RowData,
  type RowSelectionState,
  type Table as TanstackTable,
  useReactTable,
} from "@tanstack/react-table";
import { ChevronDown, ChevronUp } from "lucide-react";
import { Checkbox } from "@/components/ui/checkbox";
import { cn } from "@/lib/cn";

export type { ColumnDef } from "@tanstack/react-table";

interface DataTableSelectionLabels {
  selectAll: string;
  selectRow: string;
}

interface DataTableProps<TData> {
  columns: ColumnDef<TData, unknown>[];
  data: TData[];
  /** Stable row key — defaults to `id` field if present, else array index. */
  rowId?: (row: TData, index: number) => string;
  /** Active sort state — `[]` for unsorted. */
  sort?: ColumnSort[];
  onSortChange?: (next: ColumnSort[]) => void;
  /** Selected row ids — `Set<string>` for O(1) toggle. */
  selectedIds?: Set<string>;
  onSelectionChange?: (next: Set<string>) => void;
  /** Predicate gating which rows are eligible for selection. */
  isRowSelectable?: (row: TData) => boolean;
  /** Localised aria labels for selection checkboxes — required when selection is on. */
  selectionLabels?: DataTableSelectionLabels;
  /** Optional minimum table width — used to keep horizontal scroll smooth. */
  minWidth?: string;
  /** Render slot for the row when expanded — when omitted, rows aren't expandable. */
  renderRowExpansion?: (row: TData) => ReactNode;
  /** Maps a row id to its expanded state. */
  expandedRowId?: string | null;
  /** Pre-row click handler — receives the row data. */
  onRowClick?: (row: TData) => void;
  /** Renders below the last row when there's nothing to show. */
  emptyState?: ReactNode;
  /** Aria-label for the underlying scroll region. */
  ariaLabel?: string;
}

export function DataTable<TData>({
  columns,
  data,
  rowId,
  sort,
  onSortChange,
  selectedIds,
  onSelectionChange,
  isRowSelectable,
  selectionLabels,
  minWidth = "640px",
  renderRowExpansion,
  expandedRowId,
  onRowClick,
  emptyState,
  ariaLabel,
}: DataTableProps<TData>) {
  const selectionEnabled = !!onSelectionChange;
  const idFor = rowId ?? defaultRowId;

  // Dev-mode contract guard: enabling selection without supplying
  // localised aria labels silently drops the checkbox column, which
  // is hard to debug from the call site. Surface the misconfiguration
  // immediately so the i18n contract is enforced at integration time.
  useEffect(() => {
    if (
      process.env.NODE_ENV !== "production" &&
      selectionEnabled &&
      !selectionLabels
    ) {
      console.warn(
        "[DataTable] `onSelectionChange` set without `selectionLabels` — " +
          "row selection is disabled. Pass `selectionLabels={{ selectAll, selectRow }}` " +
          "with i18n strings to enable.",
      );
    }
  }, [selectionEnabled, selectionLabels]);

  const allColumns = useMemo<ColumnDef<TData, unknown>[]>(() => {
    if (!selectionEnabled || !selectionLabels) return columns;
    return [
      selectionColumn<TData>(isRowSelectable, selectionLabels),
      ...columns,
    ];
  }, [columns, selectionEnabled, isRowSelectable, selectionLabels]);

  const rowSelection = useMemo<RowSelectionState>(() => {
    if (!selectedIds) return {};
    const map: RowSelectionState = {};
    for (const id of selectedIds) map[id] = true;
    return map;
  }, [selectedIds]);

  const table = useReactTable<TData>({
    data,
    columns: allColumns,
    state: {
      sorting: sort,
      rowSelection,
    },
    enableRowSelection: selectionEnabled
      ? isRowSelectable
        ? (row) => isRowSelectable(row.original)
        : true
      : false,
    enableMultiRowSelection: true,
    getRowId: (row, index) => idFor(row, index),
    onSortingChange: (updater) => {
      if (!onSortChange) return;
      const next = typeof updater === "function" ? updater(sort ?? []) : updater;
      onSortChange(next);
    },
    onRowSelectionChange: (updater) => {
      if (!onSelectionChange) return;
      const nextMap =
        typeof updater === "function" ? updater(rowSelection) : updater;
      const ids = new Set<string>();
      for (const [k, v] of Object.entries(nextMap)) {
        if (v) ids.add(k);
      }
      onSelectionChange(ids);
    },
    getCoreRowModel: getCoreRowModel(),
    getSortedRowModel: getSortedRowModel(),
  });

  const headerGroups = table.getHeaderGroups();
  const rows = table.getRowModel().rows;

  if (rows.length === 0 && emptyState) {
    return <>{emptyState}</>;
  }

  return (
    <div
      tabIndex={0}
      role="region"
      aria-label={ariaLabel}
      className="-mx-6 overflow-x-auto px-6 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand-foreground/40"
    >
      <table
        className="w-full text-xs"
        style={{ minWidth }}
      >
        <thead>
          {headerGroups.map((group) => (
            <tr
              key={group.id}
              className="border-b border-divider text-start text-2xs uppercase tracking-wider text-foreground-muted"
            >
              {group.headers.map((header) => {
                const sortable = header.column.getCanSort();
                const sortDir = header.column.getIsSorted();
                return (
                  <th
                    key={header.id}
                    className={cn(
                      "py-2 pe-4 font-medium",
                      header.column.columnDef.meta?.headerClass,
                      sortable && "cursor-pointer select-none",
                    )}
                    onClick={
                      sortable
                        ? header.column.getToggleSortingHandler()
                        : undefined
                    }
                    aria-sort={
                      sortDir === "asc"
                        ? "ascending"
                        : sortDir === "desc"
                          ? "descending"
                          : undefined
                    }
                  >
                    <span className="inline-flex items-center gap-1">
                      {flexRender(
                        header.column.columnDef.header,
                        header.getContext(),
                      )}
                      {sortable && (
                        <span aria-hidden className="text-foreground-subtle">
                          {sortDir === "asc" ? (
                            <ChevronUp className="h-3 w-3" />
                          ) : sortDir === "desc" ? (
                            <ChevronDown className="h-3 w-3" />
                          ) : (
                            <span className="inline-block h-3 w-3" />
                          )}
                        </span>
                      )}
                    </span>
                  </th>
                );
              })}
            </tr>
          ))}
        </thead>
        <tbody>
          {rows.map((row) => {
            const isExpanded = expandedRowId === row.id;
            const expandable = !!renderRowExpansion;
            return (
              <RowFragment
                key={row.id}
                row={row}
                isSelected={selectedIds?.has(row.id) ?? false}
                isExpanded={isExpanded}
                expandable={expandable}
                onRowClick={onRowClick}
                expansion={
                  isExpanded ? renderRowExpansion?.(row.original) : undefined
                }
                table={table}
              />
            );
          })}
        </tbody>
      </table>
    </div>
  );
}

function RowFragment<TData>({
  row,
  isSelected,
  isExpanded,
  expandable,
  onRowClick,
  expansion,
  table,
}: {
  row: Row<TData>;
  isSelected: boolean;
  isExpanded: boolean;
  expandable: boolean;
  onRowClick?: (row: TData) => void;
  expansion: ReactNode | undefined;
  table: TanstackTable<TData>;
}) {
  const visibleCount = table.getVisibleLeafColumns().length;
  const interactive = !!onRowClick;
  const activate = onRowClick ? () => onRowClick(row.original) : undefined;
  return (
    <>
      <tr
        className={cn(
          "border-b border-divider-soft transition-colors duration-[var(--duration-quick)]",
          interactive &&
            "cursor-pointer hover:bg-surface-raised focus:bg-surface-raised focus:outline-none focus-visible:ring-2 focus-visible:ring-brand-foreground/60",
          isSelected && "bg-brand-surface/30",
        )}
        onClick={activate}
        // Row activation lifts to the keyboard plane when interactive —
        // Tab places focus on the row, Enter / Space toggles. A real
        // <button> can't wrap a <tr>, so the keyboard semantics live on
        // the row itself. WCAG 2.1.1 (Keyboard) + 4.1.2 (Name, Role,
        // Value) compliance.
        {...(interactive
          ? {
              role: "button",
              tabIndex: 0,
              "aria-expanded": expandable ? isExpanded : undefined,
              onKeyDown: (e: React.KeyboardEvent) => {
                if (e.key === "Enter" || e.key === " ") {
                  e.preventDefault();
                  activate?.();
                }
              },
            }
          : {})}
      >
        {row.getVisibleCells().map((cell) => (
          <td
            key={cell.id}
            className={cn(
              "py-2 pe-4",
              cell.column.columnDef.meta?.cellClass,
            )}
          >
            {flexRender(cell.column.columnDef.cell, cell.getContext())}
          </td>
        ))}
      </tr>
      {expansion && (
        <tr className="border-b border-divider-soft">
          <td colSpan={visibleCount} className="bg-surface-raised px-4 py-3">
            {expansion}
          </td>
        </tr>
      )}
    </>
  );
}

function selectionColumn<TData>(
  isRowSelectable: ((row: TData) => boolean) | undefined,
  labels: DataTableSelectionLabels,
): ColumnDef<TData, unknown> {
  return {
    id: "__select",
    enableSorting: false,
    meta: { headerClass: "w-8", cellClass: "w-8 align-middle" },
    header: ({ table }) => {
      const selectableRows = table
        .getRowModel()
        .rows.filter((r) =>
          isRowSelectable ? isRowSelectable(r.original) : true,
        );
      const allSelected =
        selectableRows.length > 0 &&
        selectableRows.every((r) => r.getIsSelected());
      const someSelected =
        !allSelected && selectableRows.some((r) => r.getIsSelected());
      return (
        <Checkbox
          checked={allSelected}
          indeterminate={someSelected}
          onChange={() => table.toggleAllRowsSelected(!allSelected)}
          aria-label={labels.selectAll}
        />
      );
    },
    cell: ({ row }) => {
      const selectable = isRowSelectable
        ? isRowSelectable(row.original)
        : true;
      if (!selectable) return null;
      return (
        <Checkbox
          checked={row.getIsSelected()}
          onChange={() => row.toggleSelected()}
          aria-label={labels.selectRow}
        />
      );
    },
  };
}

function defaultRowId<TData>(row: TData, index: number): string {
  if (
    row &&
    typeof row === "object" &&
    "id" in row &&
    typeof (row as { id?: unknown }).id === "string"
  ) {
    return (row as { id: string }).id;
  }
  return String(index);
}

declare module "@tanstack/react-table" {
  // Augment the column meta with our cell/header class slots so call
  // sites can pin column-level styling without dropping into a
  // separate column-config object.
  interface ColumnMeta<TData extends RowData, TValue> {
    headerClass?: string;
    cellClass?: string;
  }
}
