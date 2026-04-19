// ---------------------------------------------------------------------------
// ir-builder.ts — Converts visual query pattern to QueryIR JSON
// ---------------------------------------------------------------------------
// Generates QueryIR matching the backend Rust types:
//   QueryIR { operation: QueryOp, limit, skip, order_by }
//   QueryOp::Match { patterns: [GraphPattern], filter, projections, optional, group_by }
//   GraphPattern::Node { variable, label, property_filters } (kind: "node")
//   GraphPattern::Relationship { variable, label, source, target, direction } (kind: "relationship")
//   Projection::Field { variable, field, alias } (kind: "field")
//   Projection::Variable { variable, alias } (kind: "variable")
//   Projection::Aggregation { function, argument, alias } (kind: "aggregation")
//   Expr::Property { variable, field } (kind: "property")
//   Expr::Literal { value } (kind: "literal")
//   Expr::Comparison { operator, left, right } (kind: "comparison")
// ---------------------------------------------------------------------------

import { parseFilterValue } from "@/lib/filter-value";

// ---------------------------------------------------------------------------
// Visual pattern types (internal to query builder)
// ---------------------------------------------------------------------------

export interface PatternNode {
  id: string;
  label: string;
  alias: string;
  filters: PatternFilter[];
  /**
   * Canvas position for XyFlow rendering. `undefined` means the
   * canvas has not yet placed this node (it will assign a default
   * slot on first render). Round-tripped separately from QueryIR
   * via `layout_hints` in the PatternIR wire shape.
   */
  position?: { x: number; y: number };
}

export interface PatternEdge {
  id: string;
  sourceNodeId: string;
  targetNodeId: string;
  relType: string;
  alias: string;
  filters: PatternFilter[];
}

export interface PatternFilter {
  id: string;
  property: string;
  operator: FilterOperator;
  value: string;
}

export type FilterOperator =
  | "=" | "!=" | ">" | "<" | ">=" | "<="
  | "CONTAINS" | "STARTS WITH";

/** Return-clause entry. Builder-facing view onto the backend
 *  `PatternProjection` type — aggregation + output alias broken out
 *  for the "Return" tab UI. */
export interface PatternReturnField {
  alias: string;
  property: string;
  aggregation?: Aggregation | null;
  outputAlias?: string;
}

export type Aggregation = "count" | "sum" | "avg" | "min" | "max";

/** Order-by entry. Mirrors backend `OrderClause` with the canvas's
 *  simplified shape (sort by `<alias>.<property>`). */
export interface PatternOrderClause {
  alias: string;
  property: string;
  direction: "asc" | "desc";
}

export interface VisualPattern {
  nodes: PatternNode[];
  edges: PatternEdge[];
  returnFields: PatternReturnField[];
  orderBy: PatternOrderClause[];
  limit: number | null;
}

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

/**
 * Builder-facing union for the subset of `GraphPattern` variants the
 * canvas emits. `ir-builder` discriminates on `kind` while assembling
 * QueryIR JSON — typing the array with this union (instead of
 * `unknown[]`) removes the need for `as any` narrowing.
 */
type NodePatternBuilder = {
  kind: "node";
  variable: string;
  label: string;
  property_filters: unknown[];
};

type RelationshipPatternBuilder = {
  kind: "relationship";
  variable: string;
  label: string;
  source: string;
  target: string;
  direction: "outgoing" | "incoming" | "both";
  property_filters: unknown[];
  /** Optional variable-length hop spec ({ min, max }). `null` when
   * the edge is a single direct hop — which is the canvas default. */
  var_length: null | { min: number | null; max: number | null };
};

type GraphPatternBuilder = NodePatternBuilder | RelationshipPatternBuilder;

function buildGraphPatterns(nodes: PatternNode[], edges: PatternEdge[]): GraphPatternBuilder[] {
  const patterns: GraphPatternBuilder[] = [];

  // Standalone nodes (not part of any edge)
  const connectedNodeIds = new Set<string>();
  for (const e of edges) {
    connectedNodeIds.add(e.sourceNodeId);
    connectedNodeIds.add(e.targetNodeId);
  }
  for (const node of nodes) {
    if (!connectedNodeIds.has(node.id)) {
      patterns.push({
        kind: "node",
        variable: node.alias,
        label: node.label,
        property_filters: [],
      });
    }
  }

  // Relationships (includes source/target nodes)
  for (const edge of edges) {
    const src = nodes.find((n) => n.id === edge.sourceNodeId);
    const tgt = nodes.find((n) => n.id === edge.targetNodeId);
    if (!src || !tgt) continue;

    // Ensure source and target nodes are in patterns
    if (!patterns.some((p) => p.kind === "node" && p.variable === src.alias)) {
      patterns.push({
        kind: "node",
        variable: src.alias,
        label: src.label,
        property_filters: [],
      });
    }
    if (!patterns.some((p) => p.kind === "node" && p.variable === tgt.alias)) {
      patterns.push({
        kind: "node",
        variable: tgt.alias,
        label: tgt.label,
        property_filters: [],
      });
    }

    patterns.push({
      kind: "relationship",
      variable: edge.alias,
      label: edge.relType,
      source: src.alias,
      target: tgt.alias,
      direction: "outgoing",
      property_filters: [],
      var_length: null,
    });
  }

  return patterns;
}

function buildFilter(nodes: PatternNode[], edges: PatternEdge[]): unknown | null {
  const conditions: unknown[] = [];

  for (const node of nodes) {
    for (const f of node.filters) {
      conditions.push({
        kind: "comparison",
        operator: f.operator,
        left: { kind: "property", variable: node.alias, field: f.property },
        right: { kind: "literal", value: parseFilterValue(f.value) },
      });
    }
  }
  for (const edge of edges) {
    for (const f of edge.filters) {
      conditions.push({
        kind: "comparison",
        operator: f.operator,
        left: { kind: "property", variable: edge.alias, field: f.property },
        right: { kind: "literal", value: parseFilterValue(f.value) },
      });
    }
  }

  if (conditions.length === 0) return null;
  if (conditions.length === 1) return conditions[0];
  return { kind: "and", operands: conditions };
}

function buildProjections(returnFields: PatternReturnField[]): unknown[] {
  return returnFields.map((f) => {
    if (f.property === "*") {
      return {
        kind: "variable",
        variable: f.alias,
        alias: f.outputAlias || null,
      };
    }
    if (f.aggregation) {
      return {
        kind: "aggregation",
        function: f.aggregation,
        argument: {
          kind: "field",
          variable: f.alias,
          field: f.property,
          alias: null,
        },
        alias: f.outputAlias || `${f.aggregation}_${f.alias}_${f.property}`,
        distinct: false,
      };
    }
    return {
      kind: "field",
      variable: f.alias,
      field: f.property,
      alias: f.outputAlias || null,
    };
  });
}

export function buildQueryIR(pattern: VisualPattern): unknown {
  const patterns = buildGraphPatterns(pattern.nodes, pattern.edges);
  const filter = buildFilter(pattern.nodes, pattern.edges);
  const projections = buildProjections(pattern.returnFields);

  const operation = {
    op: "match",
    patterns,
    filter,
    projections,
    optional: false,
    group_by: [],
  };

  const order_by = pattern.orderBy.map((ob) => ({
    projection: {
      kind: "field",
      variable: ob.alias,
      field: ob.property,
      alias: null,
    },
    direction: ob.direction,
  }));

  return {
    operation,
    limit: pattern.limit,
    skip: null,
    order_by,
  };
}

// ---------------------------------------------------------------------------
// Inline validation
// ---------------------------------------------------------------------------
//
// `validatePattern` runs on the canvas state before compile. It surfaces
// three classes of problems:
//   - `canvas-empty` / `filter-missing-property` — authoring mistakes
//     that stop compilation.
//   - `unknown-label` / `unknown-edge-type` — the pattern references
//     an ontology entity that isn't in the current ontology snapshot.
//   - `filter-orphan` — a filter's property name doesn't exist on the
//     referenced node/edge type (best-effort, skipped when the
//     ontology snapshot is not provided).
//
// Each issue is tied to an element id (`node.id` or `edge.id`) so the
// canvas can paint a red ring on the offending element without another
// round-trip to the backend. The `severity` keeps `info` for
// non-blocking hints ("auto-return will be applied") so the panel can
// render them in a separate channel.

export type PatternIssueSeverity = "error" | "warning" | "info";

export interface PatternIssue {
  severity: PatternIssueSeverity;
  /** Canvas element the issue is anchored to. `null` means the whole
   *  pattern / return clause. */
  elementId: string | null;
  /** Short, user-facing message. Kept locale-neutral for now — Phase
   *  2-3 scope was the global shell; panel copy gets i18n'd alongside
   *  the rest of the query builder in a follow-up. */
  message: string;
  /** Stable code so downstream UI can change copy without breaking
   *  callers that key on the reason. */
  code:
    | "canvas-empty"
    | "node-missing-label"
    | "edge-missing-type"
    | "edge-dangling"
    | "filter-missing-property"
    | "filter-unknown-property"
    | "unknown-label"
    | "unknown-edge-type"
    | "return-empty";
}

export interface ValidatedPattern {
  issues: PatternIssue[];
  /** Element ids with at least one error. Canvas renders a red ring
   *  around these; useful as a `Set.has(id)` lookup in render. */
  errorIds: Set<string>;
}

interface OntologySnapshot {
  node_types: ReadonlyArray<{ label: string; properties: ReadonlyArray<{ name: string }> }>;
  edge_types: ReadonlyArray<{ label: string; properties: ReadonlyArray<{ name: string }> }>;
}

export function validatePattern(
  pattern: VisualPattern,
  ontology?: OntologySnapshot | null,
): ValidatedPattern {
  const issues: PatternIssue[] = [];

  // Whole-canvas checks
  if (pattern.nodes.length === 0 && pattern.edges.length === 0) {
    issues.push({
      severity: "info",
      elementId: null,
      code: "canvas-empty",
      message: "Drop a node type from the palette to start building a query.",
    });
  }

  // Per-node checks
  const nodesById = new Map(pattern.nodes.map((n) => [n.id, n]));
  const labelSet = ontology
    ? new Set(ontology.node_types.map((t) => t.label))
    : null;
  const propertyByLabel = ontology
    ? new Map(
        ontology.node_types.map(
          (t): [string, Set<string>] => [t.label, new Set(t.properties.map((p) => p.name))],
        ),
      )
    : null;

  for (const node of pattern.nodes) {
    if (!node.label) {
      issues.push({
        severity: "error",
        elementId: node.id,
        code: "node-missing-label",
        message: `Node "${node.alias}" is missing a label.`,
      });
      continue;
    }
    if (labelSet && !labelSet.has(node.label)) {
      issues.push({
        severity: "error",
        elementId: node.id,
        code: "unknown-label",
        message: `Label "${node.label}" isn't in the current ontology.`,
      });
    }
    for (const f of node.filters) {
      if (!f.property) {
        issues.push({
          severity: "error",
          elementId: node.id,
          code: "filter-missing-property",
          message: `Filter on "${node.alias}" is missing a property name.`,
        });
        continue;
      }
      const knownProps = propertyByLabel?.get(node.label);
      if (knownProps && !knownProps.has(f.property)) {
        issues.push({
          severity: "warning",
          elementId: node.id,
          code: "filter-unknown-property",
          message: `Property "${f.property}" is not defined on "${node.label}".`,
        });
      }
    }
  }

  // Per-edge checks
  const edgeTypeSet = ontology
    ? new Set(ontology.edge_types.map((t) => t.label))
    : null;
  const edgePropertyByLabel = ontology
    ? new Map(
        ontology.edge_types.map(
          (t): [string, Set<string>] => [t.label, new Set(t.properties.map((p) => p.name))],
        ),
      )
    : null;

  for (const edge of pattern.edges) {
    if (!edge.relType) {
      issues.push({
        severity: "error",
        elementId: edge.id,
        code: "edge-missing-type",
        message: `Edge "${edge.alias}" is missing a relationship type.`,
      });
    } else if (edgeTypeSet && !edgeTypeSet.has(edge.relType)) {
      issues.push({
        severity: "error",
        elementId: edge.id,
        code: "unknown-edge-type",
        message: `Edge type "${edge.relType}" isn't in the current ontology.`,
      });
    }
    if (!nodesById.has(edge.sourceNodeId) || !nodesById.has(edge.targetNodeId)) {
      issues.push({
        severity: "error",
        elementId: edge.id,
        code: "edge-dangling",
        message: `Edge "${edge.alias}" references a deleted node.`,
      });
    }
    for (const f of edge.filters) {
      if (!f.property) {
        issues.push({
          severity: "error",
          elementId: edge.id,
          code: "filter-missing-property",
          message: `Filter on "${edge.alias}" is missing a property name.`,
        });
        continue;
      }
      const knownProps = edgePropertyByLabel?.get(edge.relType);
      if (knownProps && !knownProps.has(f.property)) {
        issues.push({
          severity: "warning",
          elementId: edge.id,
          code: "filter-unknown-property",
          message: `Property "${f.property}" is not defined on "${edge.relType}".`,
        });
      }
    }
  }

  // Return clause — info only; builder auto-returns all aliases when
  // none are explicitly picked.
  if (pattern.returnFields.length === 0 && pattern.nodes.length > 0) {
    issues.push({
      severity: "info",
      elementId: null,
      code: "return-empty",
      message: "No explicit return fields — all node aliases will be returned.",
    });
  }

  const errorIds = new Set<string>();
  for (const issue of issues) {
    if (issue.severity === "error" && issue.elementId) {
      errorIds.add(issue.elementId);
    }
  }

  return { issues, errorIds };
}

// ---------------------------------------------------------------------------
// Preview: human-readable pseudo-Cypher
// ---------------------------------------------------------------------------

export function previewCypher(pattern: VisualPattern): string {
  const lines: string[] = [];

  if (pattern.edges.length > 0) {
    for (const edge of pattern.edges) {
      const src = pattern.nodes.find((n) => n.id === edge.sourceNodeId);
      const tgt = pattern.nodes.find((n) => n.id === edge.targetNodeId);
      if (!src || !tgt) continue;
      lines.push(
        `MATCH (${src.alias}:${src.label})-[${edge.alias}:${edge.relType}]->(${tgt.alias}:${tgt.label})`,
      );
    }
    // Standalone nodes
    const connected = new Set<string>();
    pattern.edges.forEach((e) => { connected.add(e.sourceNodeId); connected.add(e.targetNodeId); });
    for (const n of pattern.nodes) {
      if (!connected.has(n.id)) lines.push(`MATCH (${n.alias}:${n.label})`);
    }
  } else {
    for (const node of pattern.nodes) {
      lines.push(`MATCH (${node.alias}:${node.label})`);
    }
  }

  const allFilters: string[] = [];
  for (const n of pattern.nodes) for (const f of n.filters)
    allFilters.push(`${n.alias}.${f.property} ${f.operator} ${JSON.stringify(parseFilterValue(f.value))}`);
  for (const e of pattern.edges) for (const f of e.filters)
    allFilters.push(`${e.alias}.${f.property} ${f.operator} ${JSON.stringify(parseFilterValue(f.value))}`);
  if (allFilters.length > 0) lines.push(`WHERE ${allFilters.join(" AND ")}`);

  if (pattern.returnFields.length > 0) {
    const parts = pattern.returnFields.map((f) => {
      if (f.property === "*") return f.alias;
      const prop = `${f.alias}.${f.property}`;
      return f.aggregation ? `${f.aggregation}(${prop})` : prop;
    });
    lines.push(`RETURN ${parts.join(", ")}`);
  } else if (pattern.nodes.length > 0) {
    // Auto-return all node aliases when no explicit return fields
    lines.push(`RETURN ${pattern.nodes.map((n) => n.alias).join(", ")}`);
  }

  if (pattern.orderBy.length > 0) {
    lines.push(`ORDER BY ${pattern.orderBy.map((o) => `${o.alias}.${o.property} ${o.direction.toUpperCase()}`).join(", ")}`);
  }

  if (pattern.limit != null) lines.push(`LIMIT ${pattern.limit}`);

  return lines.join("\n");
}
