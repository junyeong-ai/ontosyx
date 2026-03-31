"use client";

import { useCallback } from "react";
import type { PatternNode, PatternEdge, ReturnField, Aggregation, OrderByField } from "./ir-builder";
import type { NodeTypeDef, EdgeTypeDef } from "@/types/api";

// ---------------------------------------------------------------------------
// ReturnSelector — RETURN clause configurator
// ---------------------------------------------------------------------------

interface ReturnSelectorProps {
  patternNodes: PatternNode[];
  patternEdges: PatternEdge[];
  /** Full ontology types for property lookup */
  nodeTypes: NodeTypeDef[];
  edgeTypes: EdgeTypeDef[];
  returnFields: ReturnField[];
  onReturnFieldsChange: (fields: ReturnField[]) => void;
  orderBy: OrderByField[];
  onOrderByChange: (orderBy: OrderByField[]) => void;
  limit: number | null;
  onLimitChange: (limit: number | null) => void;
}

const AGGREGATIONS: { value: Aggregation | ""; label: string }[] = [
  { value: "", label: "None" },
  { value: "count", label: "COUNT" },
  { value: "sum", label: "SUM" },
  { value: "avg", label: "AVG" },
  { value: "min", label: "MIN" },
  { value: "max", label: "MAX" },
];

export function ReturnSelector({
  patternNodes,
  patternEdges,
  nodeTypes,
  edgeTypes,
  returnFields,
  onReturnFieldsChange,
  orderBy,
  onOrderByChange,
  limit,
  onLimitChange,
}: ReturnSelectorProps) {
  // Collect all available properties grouped by alias
  const groups = buildPropertyGroups(patternNodes, patternEdges, nodeTypes, edgeTypes);

  const isChecked = useCallback(
    (alias: string, property: string) =>
      returnFields.some((f) => f.alias === alias && f.property === property),
    [returnFields],
  );

  const toggleField = useCallback(
    (alias: string, property: string) => {
      const exists = returnFields.find(
        (f) => f.alias === alias && f.property === property,
      );
      if (exists) {
        onReturnFieldsChange(returnFields.filter((f) => f !== exists));
        // Also remove from orderBy if present
        onOrderByChange(
          orderBy.filter((o) => !(o.alias === alias && o.property === property)),
        );
      } else {
        onReturnFieldsChange([
          ...returnFields,
          { alias, property, aggregation: null },
        ]);
      }
    },
    [returnFields, onReturnFieldsChange, orderBy, onOrderByChange],
  );

  const setAggregation = useCallback(
    (alias: string, property: string, agg: Aggregation | null) => {
      onReturnFieldsChange(
        returnFields.map((f) =>
          f.alias === alias && f.property === property
            ? { ...f, aggregation: agg }
            : f,
        ),
      );
    },
    [returnFields, onReturnFieldsChange],
  );

  const toggleOrderBy = useCallback(
    (alias: string, property: string) => {
      const idx = orderBy.findIndex(
        (o) => o.alias === alias && o.property === property,
      );
      if (idx >= 0) {
        // Cycle: asc -> desc -> remove
        const current = orderBy[idx];
        if (current.direction === "asc") {
          const next = [...orderBy];
          next[idx] = { ...current, direction: "desc" };
          onOrderByChange(next);
        } else {
          onOrderByChange(orderBy.filter((_, i) => i !== idx));
        }
      } else {
        onOrderByChange([...orderBy, { alias, property, direction: "asc" }]);
      }
    },
    [orderBy, onOrderByChange],
  );

  const getOrderDir = useCallback(
    (alias: string, property: string) => {
      const entry = orderBy.find(
        (o) => o.alias === alias && o.property === property,
      );
      return entry?.direction ?? null;
    },
    [orderBy],
  );

  return (
    <div className="space-y-3">
      {/* RETURN fields */}
      <div>
        <span className="text-[10px] font-semibold uppercase tracking-wider text-zinc-400">
          Return Fields
        </span>

        {groups.length === 0 && (
          <p className="mt-1 text-[11px] text-zinc-400">
            Add nodes to the pattern to select return fields.
          </p>
        )}

        {groups.map((group) => (
          <div key={group.alias} className="mt-2">
            <span className="text-[11px] font-medium text-zinc-600 dark:text-zinc-300">
              {group.alias}{" "}
              <span className="text-zinc-400">:{group.label}</span>
            </span>
            <div className="mt-1 space-y-0.5">
              {group.properties.map((prop) => {
                const checked = isChecked(group.alias, prop);
                const field = returnFields.find(
                  (f) => f.alias === group.alias && f.property === prop,
                );
                const dir = getOrderDir(group.alias, prop);
                return (
                  <div key={prop} className="flex items-center gap-2">
                    <label className="flex flex-1 cursor-pointer items-center gap-1.5">
                      <input
                        type="checkbox"
                        checked={checked}
                        onChange={() => toggleField(group.alias, prop)}
                        className="h-3 w-3 rounded border-zinc-300 text-emerald-600 focus:ring-emerald-500 dark:border-zinc-600"
                      />
                      <span className="text-xs text-zinc-700 dark:text-zinc-300">
                        {prop}
                      </span>
                    </label>

                    {checked && (
                      <>
                        {/* Aggregation */}
                        <select
                          value={field?.aggregation ?? ""}
                          onChange={(e) =>
                            setAggregation(
                              group.alias,
                              prop,
                              (e.target.value as Aggregation) || null,
                            )
                          }
                          className="h-6 rounded border border-zinc-200 bg-white px-1 text-[10px] text-zinc-600 dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-400"
                        >
                          {AGGREGATIONS.map((a) => (
                            <option key={a.value} value={a.value}>
                              {a.label}
                            </option>
                          ))}
                        </select>

                        {/* Order toggle */}
                        <button
                          onClick={() => toggleOrderBy(group.alias, prop)}
                          className={`h-6 rounded px-1.5 text-[10px] font-medium transition-colors ${
                            dir
                              ? "bg-emerald-50 text-emerald-600 dark:bg-emerald-950 dark:text-emerald-400"
                              : "text-zinc-400 hover:text-zinc-600 dark:hover:text-zinc-300"
                          }`}
                          title="Toggle sort order"
                        >
                          {dir === "asc" ? "ASC" : dir === "desc" ? "DESC" : "Sort"}
                        </button>
                      </>
                    )}
                  </div>
                );
              })}
            </div>
          </div>
        ))}
      </div>

      {/* LIMIT */}
      <div>
        <label className="flex items-center gap-2">
          <span className="text-[10px] font-semibold uppercase tracking-wider text-zinc-400">
            Limit
          </span>
          <input
            type="number"
            min={1}
            max={10000}
            value={limit ?? ""}
            onChange={(e) => {
              const v = e.target.value;
              onLimitChange(v ? parseInt(v, 10) : null);
            }}
            placeholder="no limit"
            className="h-7 w-24 rounded border border-zinc-200 bg-white px-2 text-xs text-zinc-700 dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-300"
          />
        </label>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

interface PropertyGroup {
  alias: string;
  label: string;
  properties: string[];
}

function buildPropertyGroups(
  nodes: PatternNode[],
  edges: PatternEdge[],
  nodeTypes: NodeTypeDef[],
  edgeTypes: EdgeTypeDef[],
): PropertyGroup[] {
  const groups: PropertyGroup[] = [];

  for (const node of nodes) {
    const typeDef = nodeTypes.find((nt) => nt.label === node.label);
    const props = typeDef?.properties.map((p) => p.name) ?? [];
    groups.push({ alias: node.alias, label: node.label, properties: props });
  }

  for (const edge of edges) {
    const typeDef = edgeTypes.find((et) => et.label === edge.relType);
    const props = typeDef?.properties.map((p) => p.name) ?? [];
    if (props.length > 0) {
      groups.push({ alias: edge.alias, label: edge.relType, properties: props });
    }
  }

  return groups;
}
