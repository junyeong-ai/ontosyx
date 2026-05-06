// Cypher autocompletion source for the `<CodeEditor language="cypher">`
// surface.
//
// The editor accepts a `cypherCompletions` prop carrying the
// workspace ontology's labels + property names. We turn that into a
// CodeMirror `CompletionSource` that suggests:
//
//   1. Cypher keywords — language-level constructs (MATCH, WHERE, ...).
//   2. Node labels — emit them after `:` so `(n:|)` triggers a
//      label-only menu.
//   3. Edge labels — emit them after `:` inside `[]` so `[r:|]`
//      triggers an edge-label menu.
//   4. Property names — emit them after `.` (e.g. `n.|`).
//
// The context-aware filtering (label-only after `:` etc.) keeps the
// menu signal-rich; an unconstrained autocomplete that always shows
// every identifier is louder than the schema-aware version
// platforms like Neo4j Browser ship.

import type {
  Completion,
  CompletionContext,
  CompletionResult,
} from "@codemirror/autocomplete";

/** Catalog of identifiers the autocomplete can suggest. Built from
 *  the workspace ontology at the call site so the editor primitive
 *  stays decoupled from the ontology shape. */
export interface CypherAutocompleteCatalog {
  /** Node-type labels (e.g. `Customer`, `Order`). */
  nodeLabels: readonly string[];
  /** Edge-type labels (e.g. `PLACED`, `CONTAINS`). */
  edgeLabels: readonly string[];
  /** Property names across every node type (deduped at the call
   *  site — the autocomplete renders the union). */
  propertyNames: readonly string[];
}

/** Cypher language keywords. The same set the highlighter knows
 *  about, exposed for autocomplete + uppercase rendering. Operators
 *  type lowercase but the convention is uppercase emission, so the
 *  completion `apply` is uppercase even when the typed prefix is
 *  lowercase. */
const CYPHER_KEYWORDS: readonly string[] = [
  "MATCH",
  "OPTIONAL",
  "WHERE",
  "RETURN",
  "WITH",
  "UNWIND",
  "ORDER BY",
  "ASC",
  "DESC",
  "LIMIT",
  "SKIP",
  "AS",
  "AND",
  "OR",
  "NOT",
  "IN",
  "IS NULL",
  "IS NOT NULL",
  "TRUE",
  "FALSE",
  "CASE",
  "WHEN",
  "THEN",
  "ELSE",
  "END",
  "CALL",
  "YIELD",
  "DISTINCT",
  "COUNT",
  "COLLECT",
  "EXISTS",
  "ANY",
  "ALL",
  "NONE",
  "SINGLE",
  "SIZE",
  "CREATE",
  "MERGE",
  "DELETE",
  "DETACH DELETE",
  "SET",
  "REMOVE",
];

/** The "trigger context" — what the cursor is preceded by drives
 *  which slice of the catalog the menu surfaces. Computed once per
 *  autocomplete invocation so the body of `cypherCompletionSource`
 *  stays a flat switch. */
type CursorContext =
  | { kind: "node-label"; from: number; prefix: string }
  | { kind: "edge-label"; from: number; prefix: string }
  | { kind: "property"; from: number; prefix: string }
  | { kind: "any"; from: number; prefix: string }
  | null;

function readCursorContext(ctx: CompletionContext): CursorContext {
  // `matchBefore` returns null when the regex doesn't anchor at the
  // cursor. We probe most-specific-first so a `[r:` lands on
  // edge-label even though it would match the bare `:` shape too.

  // Edge label — `[var:` or `[:` (bracket open before `:`).
  const edgeMatch = ctx.matchBefore(/\[[A-Za-z_][A-Za-z0-9_]*\s*:[A-Za-z0-9_]*$|\[:[A-Za-z0-9_]*$/);
  if (edgeMatch) {
    const colonIdx = edgeMatch.text.lastIndexOf(":");
    return {
      kind: "edge-label",
      from: edgeMatch.from + colonIdx + 1,
      prefix: edgeMatch.text.slice(colonIdx + 1),
    };
  }

  // Node label — `(var:` or `(:` (paren open before `:`).
  const nodeMatch = ctx.matchBefore(/\([A-Za-z_][A-Za-z0-9_]*\s*:[A-Za-z0-9_]*$|\(:[A-Za-z0-9_]*$/);
  if (nodeMatch) {
    const colonIdx = nodeMatch.text.lastIndexOf(":");
    return {
      kind: "node-label",
      from: nodeMatch.from + colonIdx + 1,
      prefix: nodeMatch.text.slice(colonIdx + 1),
    };
  }

  // Property — `var.prefix`. Variable name + dot + zero-or-more
  // identifier chars.
  const propMatch = ctx.matchBefore(/[A-Za-z_][A-Za-z0-9_]*\.[A-Za-z0-9_]*$/);
  if (propMatch) {
    const dotIdx = propMatch.text.lastIndexOf(".");
    return {
      kind: "property",
      from: propMatch.from + dotIdx + 1,
      prefix: propMatch.text.slice(dotIdx + 1),
    };
  }

  // Generic identifier — keyword menu.
  const wordMatch = ctx.matchBefore(/[A-Za-z_][A-Za-z0-9_]*$/);
  if (wordMatch && wordMatch.text.length > 0) {
    return {
      kind: "any",
      from: wordMatch.from,
      prefix: wordMatch.text,
    };
  }

  // Cursor at empty position. Show keyword menu only when the user
  // explicitly invoked autocomplete (Ctrl-Space); don't surface on
  // typing whitespace.
  if (ctx.explicit) {
    return { kind: "any", from: ctx.pos, prefix: "" };
  }
  return null;
}

/** Build a CodeMirror `CompletionSource` from the workspace
 *  ontology catalog. Returns a function the editor calls on every
 *  cursor movement / character entry. */
export function makeCypherCompletionSource(
  catalog: CypherAutocompleteCatalog,
): (ctx: CompletionContext) => CompletionResult | null {
  const nodeLabelOptions: Completion[] = catalog.nodeLabels.map((label) => ({
    label,
    type: "class",
    boost: 1,
  }));
  const edgeLabelOptions: Completion[] = catalog.edgeLabels.map((label) => ({
    label,
    type: "namespace",
    boost: 1,
  }));
  const propertyOptions: Completion[] = catalog.propertyNames.map((name) => ({
    label: name,
    type: "property",
  }));
  // Keyword + node + edge + property all eligible from the bare
  // identifier menu so the user typing `M` sees `MATCH`, typing
  // `Cu` sees `Customer`. Properties are excluded from the bare
  // menu (a 7-property catalog × 4 node types pollutes the
  // 5-keyword menu); they surface only after a `.`.
  const bareOptions: Completion[] = [
    ...CYPHER_KEYWORDS.map((kw): Completion => ({
      label: kw,
      type: "keyword",
      boost: 2,
    })),
    ...nodeLabelOptions,
    ...edgeLabelOptions,
  ];

  return (ctx) => {
    const cursor = readCursorContext(ctx);
    if (!cursor) return null;
    let options: Completion[];
    switch (cursor.kind) {
      case "node-label":
        options = nodeLabelOptions;
        break;
      case "edge-label":
        options = edgeLabelOptions;
        break;
      case "property":
        options = propertyOptions;
        break;
      case "any":
        options = bareOptions;
        break;
    }
    return {
      from: cursor.from,
      options,
      // CodeMirror filters by prefix automatically when `validFor`
      // is a regex over the typed range — keeps the menu live as
      // the user keeps typing without re-invoking the source.
      validFor: /^[A-Za-z0-9_]*$/,
    };
  };
}

/** Build a catalog from a typed ontology shape. Pure function
 *  over the IR's `node_types` + `edge_types` collections; the
 *  caller passes whatever ontology snapshot they hold. Properties
 *  dedup across node types via a Set so the autocomplete doesn't
 *  surface 17 copies of `id`. */
export function buildCatalogFromOntology(ontology: {
  node_types?: ReadonlyArray<{
    label?: string;
    properties?: ReadonlyArray<{ name?: string }>;
  }>;
  edge_types?: ReadonlyArray<{ label?: string }>;
}): CypherAutocompleteCatalog {
  const nodeLabels = (ontology.node_types ?? [])
    .map((n) => n.label ?? "")
    .filter((l) => l.length > 0);
  const edgeLabels = (ontology.edge_types ?? [])
    .map((e) => e.label ?? "")
    .filter((l) => l.length > 0);
  const propSet = new Set<string>();
  for (const nt of ontology.node_types ?? []) {
    for (const p of nt.properties ?? []) {
      if (p.name && p.name.length > 0) propSet.add(p.name);
    }
  }
  return {
    nodeLabels,
    edgeLabels,
    propertyNames: Array.from(propSet).sort(),
  };
}
