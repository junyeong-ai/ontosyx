import { describe, expect, it } from "vitest";

import {
  buildCatalogFromOntology,
  makeCypherCompletionSource,
} from "./autocomplete";

// Lightweight CompletionContext stand-in. CodeMirror's actual
// shape needs a full EditorState; we simulate the surface area
// the source actually reads (`pos`, `matchBefore`, `explicit`).
interface MatchBeforeResult {
  from: number;
  to: number;
  text: string;
}
function makeContext(text: string, explicit = false) {
  const pos = text.length;
  return {
    pos,
    explicit,
    matchBefore(re: RegExp): MatchBeforeResult | null {
      // CodeMirror's actual `matchBefore` anchors at the cursor;
      // we mimic by sticking `$` onto the pattern and running
      // against the full `text` slice.
      const anchored = new RegExp(re.source + "$", re.flags);
      const m = text.match(anchored);
      if (!m || m.index === undefined) return null;
      return { from: m.index, to: pos, text: m[0] };
    },
    state: { doc: { length: pos } },
  };
}

describe("buildCatalogFromOntology", () => {
  it("dedupes property names across node types", () => {
    const ontology = {
      node_types: [
        {
          label: "Customer",
          properties: [{ name: "id" }, { name: "name" }],
        },
        {
          label: "Order",
          // `id` collides with Customer's `id` — should appear once.
          properties: [{ name: "id" }, { name: "total" }],
        },
      ],
      edge_types: [{ label: "PLACED" }],
    };
    const catalog = buildCatalogFromOntology(ontology);
    expect(catalog.nodeLabels).toEqual(["Customer", "Order"]);
    expect(catalog.edgeLabels).toEqual(["PLACED"]);
    expect(catalog.propertyNames).toEqual(["id", "name", "total"]);
  });

  it("survives missing fields", () => {
    const catalog = buildCatalogFromOntology({});
    expect(catalog.nodeLabels).toEqual([]);
    expect(catalog.edgeLabels).toEqual([]);
    expect(catalog.propertyNames).toEqual([]);
  });

  it("filters empty labels and property names", () => {
    const catalog = buildCatalogFromOntology({
      node_types: [
        { label: "", properties: [{ name: "" }, { name: "ok" }] },
        { label: "Real", properties: [] },
      ],
      edge_types: [{ label: "" }, { label: "ALSO_REAL" }],
    });
    expect(catalog.nodeLabels).toEqual(["Real"]);
    expect(catalog.edgeLabels).toEqual(["ALSO_REAL"]);
    expect(catalog.propertyNames).toEqual(["ok"]);
  });
});

describe("makeCypherCompletionSource", () => {
  const catalog = {
    nodeLabels: ["Customer", "Order"],
    edgeLabels: ["PLACED", "CONTAINS"],
    propertyNames: ["id", "name", "total"],
  };
  const source = makeCypherCompletionSource(catalog);

  it("surfaces node labels after `(:`", () => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const ctx = makeContext("MATCH (n:") as any;
    const result = source(ctx);
    expect(result).not.toBeNull();
    const labels = result!.options.map((o) => o.label);
    expect(labels).toContain("Customer");
    expect(labels).toContain("Order");
    // Edge labels should NOT appear in the node-label menu.
    expect(labels).not.toContain("PLACED");
    // Properties should NOT appear here either.
    expect(labels).not.toContain("id");
  });

  it("surfaces edge labels after `[r:`", () => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const ctx = makeContext("MATCH (n)-[r:") as any;
    const result = source(ctx);
    expect(result).not.toBeNull();
    const labels = result!.options.map((o) => o.label);
    expect(labels).toContain("PLACED");
    expect(labels).toContain("CONTAINS");
    expect(labels).not.toContain("Customer");
  });

  it("surfaces properties after `var.`", () => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const ctx = makeContext("MATCH (n:Customer) WHERE n.") as any;
    const result = source(ctx);
    expect(result).not.toBeNull();
    const labels = result!.options.map((o) => o.label);
    expect(labels).toContain("id");
    expect(labels).toContain("name");
    expect(labels).toContain("total");
    expect(labels).not.toContain("Customer");
  });

  it("surfaces keywords + node + edge labels in the bare identifier menu", () => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const ctx = makeContext("MA") as any;
    const result = source(ctx);
    expect(result).not.toBeNull();
    const labels = result!.options.map((o) => o.label);
    // Keywords always present so the bare menu surfaces MATCH.
    expect(labels).toContain("MATCH");
    // Node + edge labels also eligible from the bare menu so the
    // user typing `Cu` lands on Customer.
    expect(labels).toContain("Customer");
    expect(labels).toContain("PLACED");
    // Properties skip the bare menu (would pollute the keyword
    // surface).
    expect(labels).not.toContain("id");
  });

  it("returns null when the cursor sits between whitespace with no explicit invocation", () => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const ctx = makeContext("MATCH ", false) as any;
    const result = source(ctx);
    expect(result).toBeNull();
  });

  it("returns the bare menu on explicit invocation at empty position", () => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const ctx = makeContext("MATCH ", true) as any;
    const result = source(ctx);
    expect(result).not.toBeNull();
    const labels = result!.options.map((o) => o.label);
    expect(labels).toContain("MATCH");
    expect(labels).toContain("Customer");
  });
});
