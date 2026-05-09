// ---------------------------------------------------------------------------
// saved-pattern-io.ts — convert local visual state ↔ backend PatternIR JSON
// ---------------------------------------------------------------------------
//
// The canvas owns a few UI-ergonomic shapes (PatternNode with `alias`,
// PatternEdge without `direction`, flat `returnFields`/`orderBy`/`limit`)
// that don't line up 1:1 with `ox_core::pattern_ir::PatternIR` on the
// server. These helpers stitch the two sides together:
//
//   toPatternIR(local)   — canvas → wire. Each local pattern-filter
//                          becomes a top-level `PatternFilter.expr` so
//                          the round-trip preserves the exact predicate.
//                          Return fields map to `PatternProjection`;
//                          positions land on `PatternNode.position`.
//   fromPatternIR(wire)  — wire → canvas. Best-effort: anything the
//                          round-trip can't express (rare Expr shapes,
//                          non-outgoing edges) is simply dropped rather
//                          than crashing the builder on load.
//
// Filters/projections/positions all survive; layout_hints (zoom + pan)
// land on the wire too, so the caller can capture XyFlow viewport state
// before calling `toPatternIR` and restore it after `fromPatternIR`.

import { parseFilterValue, stringifyFilterValue } from "@/lib/filter-value";
import type { components } from "@/types/api.generated";
import type {
  PatternNode,
  PatternEdge,
  PatternFilter,
  PatternReturnField,
  PatternOrderClause,
  FilterOperator,
  Aggregation,
  VisualPattern,
} from "./ir-builder";
import { toComparisonOp, toPropertyValue, toStringOp } from "./ir-builder";

// ---------------------------------------------------------------------------
// Backend wire shapes (match ox_core::pattern_ir serde output)
// ---------------------------------------------------------------------------

type WireLayoutHints = components["schemas"]["LayoutHints"];
type WirePatternNode = components["schemas"]["PatternNode"];
type WirePatternEdge = components["schemas"]["PatternEdge"];
type WirePatternFilter = components["schemas"]["PatternFilter"];
type WirePatternProjection = components["schemas"]["PatternProjection"];
type WireOrderClause = components["schemas"]["OrderClause"];
type WireExpr = components["schemas"]["Expr"];
type WirePropertyValue = components["schemas"]["PropertyValue"];

/** On-wire shape version — mirrors the Rust `PATTERN_IR_SCHEMA_VERSION`
 *  constant. The backend rejects a higher value during deserialisation,
 *  so bump here in lockstep whenever a breaking field shape change
 *  lands on the Rust side. Older payloads (missing the field) serde
 *  back to the default on the backend. */
export const PATTERN_IR_SCHEMA_VERSION = 1;

/** Why a decompiled PatternIR is not editable on the canvas — mirrors
 *  the Rust `pattern_ir::ReadOnlyReason`. `Some` when the backend's
 *  `decompile` ran against a non-`Match` QueryIR operation that the
 *  canvas can't round-trip; `undefined` for freshly-built patterns
 *  and for `Match` decompiles (the common editable case).
 *
 *  `original_op` is the Rust `QueryOp` variant name (`"Aggregate"`,
 *  `"Union"`, `"PathFind"`, etc.). The UI maps these to localised
 *  labels for the "not editable: <op>" banner; the wire value stays
 *  canonical so a server-side rename flags up at deserialization. */
export interface WireReadOnlyReason {
  original_op: string;
}

export type WirePatternIR = components["schemas"]["PatternIR"];

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const AGGREGATIONS: readonly Aggregation[] = ["count", "sum", "avg", "min", "max"] as const;

function isAggregation(value: unknown): value is Aggregation {
  return typeof value === "string" && (AGGREGATIONS as readonly string[]).includes(value);
}

function stringifyPropertyValue(value: WirePropertyValue | undefined): string {
  if (!value || value.type === "null") return "";
  if ("value" in value) return stringifyFilterValue(value.value);
  return "";
}

function comparisonOperator(op: components["schemas"]["ComparisonOp"]): FilterOperator {
  switch (op) {
    case "eq":
      return "=";
    case "neq":
      return "!=";
    case "gt":
      return ">";
    case "lt":
      return "<";
    case "gte":
      return ">=";
    case "lte":
      return "<=";
  }
}

function stringOperator(op: components["schemas"]["StringOp"]): Extract<FilterOperator, "CONTAINS" | "STARTS WITH"> | null {
  if (op === "contains") return "CONTAINS";
  if (op === "starts_with") return "STARTS WITH";
  return null;
}

function localFilterFromExpr(id: string, expr: WireExpr): PatternFilter | null {
  if (expr.expr_type !== "comparison" && expr.expr_type !== "string_op") return null;
  if (expr.left.expr_type !== "property" || expr.right.expr_type !== "literal") return null;
  const operator = expr.expr_type === "comparison" ? comparisonOperator(expr.op) : stringOperator(expr.op);
  if (!operator || !expr.left.field) return null;
  return {
    id: id || freshFilterId(),
    property: expr.left.field,
    operator,
    value: stringifyPropertyValue(expr.right.value),
  };
}

// Collision-free ids, same module-global across tabs. `crypto.randomUUID`
// is stable across every runtime we target (Next.js 16 SSR + evergreen
// browsers + Node ≥ 14.17), so no fallback branch is warranted.
function freshFilterId(): string {
  return `pf-${crypto.randomUUID()}`;
}

function freshProjectionId(): string {
  return `pp-${crypto.randomUUID()}`;
}

// ---------------------------------------------------------------------------
// canvas → wire
// ---------------------------------------------------------------------------

export interface ToPatternIROptions {
  layoutHints?: WireLayoutHints;
}

export function toPatternIR(
  visual: VisualPattern,
  options: ToPatternIROptions = {},
): WirePatternIR {
  const nodes: WirePatternNode[] = visual.nodes.map((n) => ({
    id: n.id,
    variable: n.alias,
    label: n.label,
    property_filters: [],
    position: n.position ? { x: n.position.x, y: n.position.y } : undefined,
  }));

  const edges: WirePatternEdge[] = visual.edges.map((e) => ({
    id: e.id,
    variable: e.alias,
    label: e.relType,
    source_node_id: e.sourceNodeId,
    target_node_id: e.targetNodeId,
    // The local canvas always stores an outgoing relationship; the
    // builder doesn't surface direction as a first-class toggle.
    direction: "outgoing" as const,
    property_filters: [],
    var_length: null,
  }));

  const filters: WirePatternFilter[] = [];
  const pushFilters = (variable: string, fs: PatternFilter[]) => {
    for (const f of fs) {
      const left: WireExpr = { expr_type: "property", variable, field: f.property };
      const right: WireExpr = { expr_type: "literal", value: toPropertyValue(parseFilterValue(f.value)) };
      filters.push({
        id: f.id,
        expr:
          f.operator === "CONTAINS" || f.operator === "STARTS WITH"
            ? { expr_type: "string_op", left, op: toStringOp(f.operator), right }
            : { expr_type: "comparison", left, op: toComparisonOp(f.operator), right },
      });
    }
  };
  for (const n of visual.nodes) pushFilters(n.alias, n.filters);
  for (const e of visual.edges) pushFilters(e.alias, e.filters);

  const projections: WirePatternProjection[] = visual.returnFields.map((rf) => {
    if (rf.aggregation) {
      return {
        id: freshProjectionId(),
        projection: {
          kind: "aggregation",
          function: rf.aggregation,
          argument: { kind: "field", variable: rf.alias, field: rf.property, alias: null },
          alias: rf.outputAlias || `${rf.aggregation}_${rf.alias}_${rf.property}`,
          distinct: false,
        },
      };
    }
    if (rf.property === "*") {
      return {
        id: freshProjectionId(),
        projection: {
          kind: "variable",
          variable: rf.alias,
          alias: rf.outputAlias,
        },
      };
    }
    return {
      id: freshProjectionId(),
      projection: {
        kind: "field",
        variable: rf.alias,
        field: rf.property,
        alias: rf.outputAlias,
      },
    };
  });

  const order_by: WireOrderClause[] = visual.orderBy.map((o) => ({
    projection: {
      kind: "field",
      variable: o.alias,
      field: o.property,
      alias: null,
    },
    direction: o.direction,
  }));

  return {
    schema_version: PATTERN_IR_SCHEMA_VERSION,
    nodes,
    edges,
    filters,
    projections,
    layout_hints: options.layoutHints ?? {},
    limit: visual.limit ?? undefined,
    skip: null,
    order_by,
  };
}

// ---------------------------------------------------------------------------
// wire → canvas
// ---------------------------------------------------------------------------

export interface FromPatternIRResult {
  visual: VisualPattern;
  layoutHints: WireLayoutHints;
  /** Pass-through of the wire-level `read_only_reason`. The canvas
   *  disables edit affordances when this is defined; the query-builder
   *  shell renders a "not editable: <op>" banner using the stored
   *  Rust variant name. */
  readOnlyReason?: WireReadOnlyReason;
}

export function fromPatternIR(wire: WirePatternIR): FromPatternIRResult {
  // ----- Nodes ---------------------------------------------------------
  const nodes: PatternNode[] = (wire.nodes ?? []).map((n) => ({
    id: n.id,
    label: n.label ?? "",
    alias: n.variable,
    filters: [],
    position: n.position ? { x: n.position.x, y: n.position.y } : undefined,
  }));

  // ----- Edges ---------------------------------------------------------
  const edges: PatternEdge[] = (wire.edges ?? []).map((e) => ({
    id: e.id,
    sourceNodeId: e.source_node_id,
    targetNodeId: e.target_node_id,
    relType: e.label ?? "",
    alias: e.variable ?? "",
    filters: [],
  }));

  // ----- Filters (distribute back onto their host variable) -----------
  const nodeByVariable = new Map(nodes.map((n) => [n.alias, n]));
  const edgeByVariable = new Map(edges.map((e) => [e.alias, e]));
  for (const f of wire.filters ?? []) {
    const expr = f.expr;
    const local = localFilterFromExpr(f.id, expr);
    if (!local) continue;
    const variable =
      (expr.expr_type === "comparison" || expr.expr_type === "string_op") &&
      expr.left.expr_type === "property"
        ? expr.left.variable
        : "";
    const host = nodeByVariable.get(variable) ?? edgeByVariable.get(variable);
    if (host) host.filters.push(local);
  }

  // ----- Return fields -------------------------------------------------
  const returnFields: PatternReturnField[] = [];
  for (const proj of wire.projections ?? []) {
    const p = proj.projection;
    if (!p) continue;
    if (p.kind === "aggregation" && isAggregation(p.function)) {
      const argument = p.argument;
      if (!argument || argument.kind !== "field") continue;
      returnFields.push({
        alias: argument.variable,
        property: argument.field,
        aggregation: p.function,
        outputAlias: p.alias ?? undefined,
      });
    } else if (p.kind === "variable") {
      returnFields.push({
        alias: p.variable,
        property: "*",
        aggregation: null,
        outputAlias: p.alias ?? undefined,
      });
    } else if (p.kind === "field") {
      returnFields.push({
        alias: p.variable,
        property: p.field,
        aggregation: null,
        outputAlias: p.alias ?? undefined,
      });
    }
  }

  // ----- Order by ------------------------------------------------------
  const orderBy: PatternOrderClause[] = [];
  for (const clause of wire.order_by ?? []) {
    const direction = clause.direction === "desc" ? "desc" : "asc";
    const p = clause.projection;
    if (!p) continue;
    if (p.kind === "field") {
      orderBy.push({ alias: p.variable, property: p.field, direction });
    } else if (p.kind === "variable") {
      // A "variable" projection sorts by the whole node; surface as
      // alias + "*" so the UI's order-by picker recognises it.
      orderBy.push({ alias: p.variable, property: "*", direction });
    }
    // Aggregation-direction sort has no canvas representation yet —
    // drop silently so a saved pattern with such a clause still loads.
  }

  return {
    visual: {
      nodes,
      edges,
      returnFields,
      orderBy,
      limit: wire.limit ?? null,
    },
    layoutHints: wire.layout_hints ?? {},
    readOnlyReason: wire.read_only_reason ?? undefined,
  };
}
