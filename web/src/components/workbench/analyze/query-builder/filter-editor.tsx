"use client";

import { useCallback } from "react";
import type { PropertyDef } from "@/types/api";
import type { PatternFilter, FilterOperator } from "./ir-builder";

// ---------------------------------------------------------------------------
// FilterEditor — WHERE clause builder for a selected node/edge
// ---------------------------------------------------------------------------

const OPERATORS: { value: FilterOperator; label: string }[] = [
  { value: "=", label: "=" },
  { value: "!=", label: "!=" },
  { value: ">", label: ">" },
  { value: "<", label: "<" },
  { value: ">=", label: ">=" },
  { value: "<=", label: "<=" },
  { value: "CONTAINS", label: "contains" },
  { value: "STARTS WITH", label: "starts with" },
];

interface FilterEditorProps {
  properties: PropertyDef[];
  filters: PatternFilter[];
  onChange: (filters: PatternFilter[]) => void;
}

export function FilterEditor({ properties, filters, onChange }: FilterEditorProps) {
  const addFilter = useCallback(() => {
    if (properties.length === 0) return;
    const newFilter: PatternFilter = {
      id: `f-${Date.now()}`,
      property: properties[0].name,
      operator: "=",
      value: "",
    };
    onChange([...filters, newFilter]);
  }, [properties, filters, onChange]);

  const updateFilter = useCallback(
    (id: string, patch: Partial<PatternFilter>) => {
      onChange(filters.map((f) => (f.id === id ? { ...f, ...patch } : f)));
    },
    [filters, onChange],
  );

  const removeFilter = useCallback(
    (id: string) => {
      onChange(filters.filter((f) => f.id !== id));
    },
    [filters, onChange],
  );

  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between">
        <span className="text-[10px] font-semibold uppercase tracking-wider text-zinc-400">
          Filters (WHERE)
        </span>
        <button
          onClick={addFilter}
          disabled={properties.length === 0}
          className="rounded px-2 py-0.5 text-[10px] font-medium text-emerald-600 transition-colors hover:bg-emerald-50 disabled:opacity-40 dark:text-emerald-400 dark:hover:bg-emerald-950"
        >
          + Add
        </button>
      </div>

      {filters.length === 0 && (
        <p className="text-[11px] text-zinc-400">No filters applied</p>
      )}

      {filters.map((filter) => (
        <div key={filter.id} className="flex items-center gap-1.5">
          {/* Property */}
          <select
            value={filter.property}
            onChange={(e) => updateFilter(filter.id, { property: e.target.value })}
            className="h-7 w-28 rounded border border-zinc-200 bg-white px-1.5 text-xs text-zinc-700 dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-300"
          >
            {properties.map((p) => (
              <option key={p.id} value={p.name}>
                {p.name}
              </option>
            ))}
          </select>

          {/* Operator */}
          <select
            value={filter.operator}
            onChange={(e) =>
              updateFilter(filter.id, {
                operator: e.target.value as FilterOperator,
              })
            }
            className="h-7 w-20 rounded border border-zinc-200 bg-white px-1.5 text-xs text-zinc-700 dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-300"
          >
            {OPERATORS.map((op) => (
              <option key={op.value} value={op.value}>
                {op.label}
              </option>
            ))}
          </select>

          {/* Value */}
          <input
            type="text"
            value={filter.value}
            onChange={(e) => updateFilter(filter.id, { value: e.target.value })}
            placeholder="value"
            className="h-7 min-w-0 flex-1 rounded border border-zinc-200 bg-white px-2 text-xs text-zinc-700 dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-300"
          />

          {/* Remove */}
          <button
            onClick={() => removeFilter(filter.id)}
            className="flex h-7 w-7 shrink-0 items-center justify-center rounded text-zinc-400 transition-colors hover:bg-red-50 hover:text-red-500 dark:hover:bg-red-950"
            title="Remove filter"
          >
            <svg className="h-3 w-3" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
              <path d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>
      ))}
    </div>
  );
}
