"use client";

import { useCallback, useMemo } from "react";
import { useTranslations } from "next-intl";
import { Checkbox } from "@/components/ui/checkbox";
import { FormInput, FormSelect } from "@/components/ui/form-input";
import type { PatternNode, PatternEdge, PatternReturnField, Aggregation, PatternOrderClause } from "./ir-builder";
import type { NodeTypeDef, EdgeTypeDef } from "@/types/api";
import { arr } from "@/lib/ir-collections";

// ---------------------------------------------------------------------------
// ReturnSelector — RETURN clause configurator
// ---------------------------------------------------------------------------

interface ReturnSelectorProps {
  patternNodes: PatternNode[];
  patternEdges: PatternEdge[];
  /** Full ontology types for property lookup */
  nodeTypes: NodeTypeDef[];
  edgeTypes: EdgeTypeDef[];
  returnFields: PatternReturnField[];
  onReturnFieldsChange: (fields: PatternReturnField[]) => void;
  orderBy: PatternOrderClause[];
  onOrderByChange: (orderBy: PatternOrderClause[]) => void;
  limit: number | null;
  onLimitChange: (limit: number | null) => void;
}

// Aggregation dropdown options. Only the "None" label is localized — the
// rest are canonical SQL-style keywords displayed as-is.
const AGGREGATION_VALUES: { value: Aggregation | ""; label: string | null }[] = [
  { value: "", label: null },
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
  const t = useTranslations("workbench.queryBuilder.return");

  // Collect all available properties grouped by alias
  const groups = buildPropertyGroups(patternNodes, patternEdges, nodeTypes, edgeTypes);

  const aggregations = useMemo(
    () =>
      AGGREGATION_VALUES.map((a) => ({
        value: a.value,
        label: a.label ?? t("aggregationNone"),
      })),
    [t],
  );

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
        <span className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
          {t("heading")}
        </span>

        {groups.length === 0 && (
          <p className="mt-1 text-2xs text-foreground-muted">
            {t("emptyHint")}
          </p>
        )}

        {groups.map((group) => (
          <div key={group.alias} className="mt-2">
            <span className="text-2xs font-medium text-foreground-muted">
              {group.alias}{" "}
              <span className="text-foreground-muted">
                {t("groupType", { label: group.label })}
              </span>
            </span>
            <div className="mt-1 space-y-0.5">
              {arr(group.properties).map((prop) => {
                const checked = isChecked(group.alias, prop);
                const field = returnFields.find(
                  (f) => f.alias === group.alias && f.property === prop,
                );
                const dir = getOrderDir(group.alias, prop);
                return (
                  <div key={prop} className="flex items-center gap-2">
                    <Checkbox
                      checked={checked}
                      onChange={() => toggleField(group.alias, prop)}
                      label={<span className="text-xs text-foreground">{prop}</span>}
                      className="flex-1"
                    />

                    {checked && (
                      <>
                        {/* Aggregation */}
                        <FormSelect
                          value={field?.aggregation ?? ""}
                          onChange={(e) =>
                            setAggregation(
                              group.alias,
                              prop,
                              (e.target.value as Aggregation) || null,
                            )
                          }
                          density="compact"
                          className="w-auto"
                        >
                          {aggregations.map((a) => (
                            <option key={a.value} value={a.value}>
                              {a.label}
                            </option>
                          ))}
                        </FormSelect>

                        {/* Order toggle */}
                        <button type="button"
                          onClick={() => toggleOrderBy(group.alias, prop)}
                          className={`h-6 rounded px-1.5 text-2xs font-medium transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] ${
                            dir
                              ? "bg-brand-surface text-brand-foreground"
                              : "text-foreground-muted hover:text-foreground-muted"
                          }`}
                          title={t("sortToggleTitle")}
                        >
                          {dir === "asc" ? t("sortAsc") : dir === "desc" ? t("sortDesc") : t("sortLabel")}
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
          <span className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
            {t("limitLabel")}
          </span>
          <FormInput
            type="number"
            min={1}
            max={10000}
            value={limit ?? ""}
            onChange={(e) => {
              const v = e.target.value;
              onLimitChange(v ? parseInt(v, 10) : null);
            }}
            placeholder={t("limitPlaceholder")}
            density="compact"
            className="w-24"
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
    const props = arr(typeDef?.properties).map((p) => p.name);
    groups.push({ alias: node.alias, label: node.label, properties: props });
  }

  for (const edge of edges) {
    const typeDef = edgeTypes.find((et) => et.label === edge.relType);
    const props = arr(typeDef?.properties).map((p) => p.name);
    if (props.length > 0) {
      groups.push({ alias: edge.alias, label: edge.relType, properties: props });
    }
  }

  return groups;
}
