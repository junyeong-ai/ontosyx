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

// ---------------------------------------------------------------------------
// Backend wire shapes (match ox_core::pattern_ir serde output)
// ---------------------------------------------------------------------------

interface WirePosition {
  x: number;
  y: number;
}

interface WireLayoutHints {
  zoom?: number;
  pan_x?: number;
  pan_y?: number;
}

interface WirePatternNode {
  id: string;
  variable: string;
  label?: string;
  property_filters?: unknown[];
  position?: WirePosition;
}

interface WirePatternEdge {
  id: string;
  variable?: string;
  label?: string;
  source_node_id: string;
  target_node_id: string;
  direction: "outgoing" | "incoming" | "both";
  property_filters?: unknown[];
  var_length?: unknown;
}

interface WireComparisonExpr {
  kind: "comparison";
  operator: FilterOperator;
  left: { kind: "property"; variable: string; field: string };
  right: { kind: "literal"; value: unknown };
}

interface WirePatternFilter {
  id: string;
  expr: WireComparisonExpr;
}

interface WireProjectionField {
  kind: "field";
  variable: string;
  field: string;
  alias?: string;
}

interface WireProjectionVariable {
  kind: "variable";
  variable: string;
  alias?: string;
}

interface WireProjectionAggregation {
  kind: "aggregation";
  function: Aggregation;
  argument: { kind: "property"; variable: string; field: string };
  alias?: string;
}

type WireProjection =
  | WireProjectionField
  | WireProjectionVariable
  | WireProjectionAggregation;

interface WirePatternProjection {
  id: string;
  projection: WireProjection;
}

/** QueryIR's `OrderClause` wire shape. `projection` wraps any
 *  `Projection` variant; the UI only emits `field` so that's what we
 *  serialize, but decoding accepts any shape and falls back to the
 *  first variable when the shape is unknown. */
interface WireOrderClause {
  projection: WireProjection;
  direction: "asc" | "desc";
}

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

export interface WirePatternIR {
  schema_version?: number;
  nodes?: WirePatternNode[];
  edges?: WirePatternEdge[];
  filters?: WirePatternFilter[];
  projections?: WirePatternProjection[];
  layout_hints?: WireLayoutHints;
  limit?: number;
  skip?: number;
  order_by?: WireOrderClause[];
  /** Present only when the server decompiled a non-`Match` QueryIR.
   *  The UI must gate edit actions on its absence — an empty
   *  `nodes` array no longer implies "blank canvas" on its own. */
  read_only_reason?: WireReadOnlyReason;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const FILTER_OPERATORS: readonly FilterOperator[] = [
  "=",
  "!=",
  ">",
  "<",
  ">=",
  "<=",
  "CONTAINS",
  "STARTS WITH",
] as const;

function isFilterOperator(value: unknown): value is FilterOperator {
  return typeof value === "string" && (FILTER_OPERATORS as readonly string[]).includes(value);
}

const AGGREGATIONS: readonly Aggregation[] = ["count", "sum", "avg", "min", "max"] as const;

function isAggregation(value: unknown): value is Aggregation {
  return typeof value === "string" && (AGGREGATIONS as readonly string[]).includes(value);
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
  }));

  const filters: WirePatternFilter[] = [];
  const pushFilters = (variable: string, fs: PatternFilter[]) => {
    for (const f of fs) {
      filters.push({
        id: f.id,
        expr: {
          kind: "comparison",
          operator: f.operator,
          left: { kind: "property", variable, field: f.property },
          right: { kind: "literal", value: parseFilterValue(f.value) },
        },
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
          argument: { kind: "property", variable: rf.alias, field: rf.property },
          alias: rf.outputAlias,
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
    if (!f.expr || f.expr.kind !== "comparison") continue;
    if (!isFilterOperator(f.expr.operator)) continue;
    const { variable, field } = f.expr.left;
    const value = stringifyFilterValue(f.expr.right?.value);
    const local: PatternFilter = {
      id: f.id || freshFilterId(),
      property: field,
      operator: f.expr.operator,
      value,
    };
    const host = nodeByVariable.get(variable) ?? edgeByVariable.get(variable);
    if (host) host.filters.push(local);
  }

  // ----- Return fields -------------------------------------------------
  const returnFields: PatternReturnField[] = [];
  for (const proj of wire.projections ?? []) {
    const p = proj.projection;
    if (!p) continue;
    if (p.kind === "aggregation" && isAggregation(p.function)) {
      returnFields.push({
        alias: p.argument?.variable ?? "",
        property: p.argument?.field ?? "",
        aggregation: p.function,
        outputAlias: p.alias,
      });
    } else if (p.kind === "variable") {
      returnFields.push({
        alias: p.variable,
        property: "*",
        aggregation: null,
        outputAlias: p.alias,
      });
    } else if (p.kind === "field") {
      returnFields.push({
        alias: p.variable,
        property: p.field,
        aggregation: null,
        outputAlias: p.alias,
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
    readOnlyReason: wire.read_only_reason,
  };
}
