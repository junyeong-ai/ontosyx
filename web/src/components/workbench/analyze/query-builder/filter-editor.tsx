"use client";

import { useCallback } from "react";
import { useTranslations } from "next-intl";
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
  const t = useTranslations("workbench.queryBuilder.filter");
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
        <span className="text-2xs font-semibold uppercase tracking-wider text-muted-foreground">
          {t("heading")}
        </span>
        <button
          onClick={addFilter}
          disabled={properties.length === 0}
          className="rounded px-2 py-0.5 text-2xs font-medium text-brand-foreground transition-colors hover:bg-brand-surface disabled:opacity-40 dark:hover:bg-brand-surface"
        >
          {t("add")}
        </button>
      </div>

      {filters.length === 0 && (
        <p className="text-[11px] text-muted-foreground">{t("empty")}</p>
      )}

      {filters.map((filter) => (
        <div key={filter.id} className="flex items-center gap-1.5">
          {/* Property */}
          <select
            value={filter.property}
            onChange={(e) => updateFilter(filter.id, { property: e.target.value })}
            className="h-7 w-28 rounded border border-divider bg-surface-base px-1.5 text-xs text-foreground-muted"
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
            className="h-7 w-20 rounded border border-divider bg-surface-base px-1.5 text-xs text-foreground-muted"
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
            placeholder={t("valuePlaceholder")}
            className="h-7 min-w-0 flex-1 rounded border border-divider bg-surface-base px-2 text-xs text-foreground-muted"
          />

          {/* Remove */}
          <button
            onClick={() => removeFilter(filter.id)}
            className="flex h-7 w-7 shrink-0 items-center justify-center rounded text-muted-foreground transition-colors hover:bg-danger-surface hover:text-danger-foreground dark:hover:bg-danger-surface"
            title={t("removeTitle")}
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
