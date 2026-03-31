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

// ---------------------------------------------------------------------------
// Visual pattern types (internal to query builder)
// ---------------------------------------------------------------------------

export interface PatternNode {
  id: string;
  label: string;
  alias: string;
  filters: PatternFilter[];
  returnProps: string[];
}

export interface PatternEdge {
  id: string;
  sourceNodeId: string;
  targetNodeId: string;
  relType: string;
  alias: string;
  filters: PatternFilter[];
  returnProps: string[];
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

export interface ReturnField {
  alias: string;
  property: string;
  aggregation?: Aggregation | null;
  outputAlias?: string;
}

export type Aggregation = "count" | "sum" | "avg" | "min" | "max";

export interface OrderByField {
  alias: string;
  property: string;
  direction: "asc" | "desc";
}

export interface VisualPattern {
  nodes: PatternNode[];
  edges: PatternEdge[];
  returnFields: ReturnField[];
  orderBy: OrderByField[];
  limit: number | null;
}

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

function parseFilterValue(raw: string): unknown {
  if (/^-?\d+(\.\d+)?$/.test(raw)) return Number(raw);
  if (raw === "true") return true;
  if (raw === "false") return false;
  if ((raw.startsWith('"') && raw.endsWith('"')) || (raw.startsWith("'") && raw.endsWith("'")))
    return raw.slice(1, -1);
  return raw;
}

function buildGraphPatterns(nodes: PatternNode[], edges: PatternEdge[]): unknown[] {
  const patterns: unknown[] = [];

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
    if (!patterns.some((p: any) => p.kind === "node" && p.variable === src.alias)) {
      patterns.push({
        kind: "node",
        variable: src.alias,
        label: src.label,
        property_filters: [],
      });
    }
    if (!patterns.some((p: any) => p.kind === "node" && p.variable === tgt.alias)) {
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

function buildProjections(returnFields: ReturnField[]): unknown[] {
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
