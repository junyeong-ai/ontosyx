import { describe, it, expect } from "vitest";

import type { OntologyIR } from "@/types/api";

import { buildOntologyIndex } from "./ontology-index";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

function ontology(): OntologyIR {
  return {
    id: "ont-1",
    name: "test",
    description: { default: "" },
    version: { number: 1 },
    node_types: [
      {
        id: "n-person",
        label: "Person",
        description: { default: "" },
        properties: [],
      },
      {
        id: "n-company",
        label: "Company",
        description: { default: "" },
        properties: [],
      },
      {
        id: "n-order",
        label: "Order",
        description: { default: "" },
        properties: [],
      },
    ],
    edge_types: [
      {
        id: "e-works-at",
        label: "WORKS_AT",
        description: { default: "" },
        source_node_id: "n-person",
        target_node_id: "n-company",
        properties: [],
        cardinality: "many_to_one",
      },
      {
        id: "e-placed",
        label: "PLACED",
        description: { default: "" },
        source_node_id: "n-person",
        target_node_id: "n-order",
        properties: [],
        cardinality: "one_to_many",
      },
    ],
  };
}

// ---------------------------------------------------------------------------
// buildOntologyIndex
// ---------------------------------------------------------------------------

describe("buildOntologyIndex — node/edge lookup", () => {
  it("indexes every node by id", () => {
    const idx = buildOntologyIndex(ontology());
    expect(idx.nodeById.size).toBe(3);
    expect(idx.nodeById.get("n-person")?.label).toBe("Person");
    expect(idx.nodeById.get("n-company")?.label).toBe("Company");
    expect(idx.nodeById.get("n-order")?.label).toBe("Order");
  });

  it("indexes every edge by id", () => {
    const idx = buildOntologyIndex(ontology());
    expect(idx.edgeById.size).toBe(2);
    expect(idx.edgeById.get("e-works-at")?.label).toBe("WORKS_AT");
    expect(idx.edgeById.get("e-placed")?.label).toBe("PLACED");
  });

  it("returns undefined for unknown ids (no fallback)", () => {
    // A lookup miss must surface as `undefined` so callers can decide
    // whether to render a placeholder or bail — the index is a pure
    // `Map`, not a best-effort resolver.
    const idx = buildOntologyIndex(ontology());
    expect(idx.nodeById.get("n-missing")).toBeUndefined();
    expect(idx.edgeById.get("e-missing")).toBeUndefined();
  });
});

describe("buildOntologyIndex — edgesByNodeId adjacency", () => {
  it("lists every edge that touches a node (as source or target)", () => {
    const idx = buildOntologyIndex(ontology());
    // n-person is the source of both edges.
    const personEdges = idx.edgesByNodeId.get("n-person") ?? [];
    expect(personEdges.map((e) => e.id).sort()).toEqual(
      ["e-placed", "e-works-at"].sort(),
    );
    // n-company is the target of exactly one edge.
    const companyEdges = idx.edgesByNodeId.get("n-company") ?? [];
    expect(companyEdges.map((e) => e.id)).toEqual(["e-works-at"]);
  });

  it("deduplicates self-referential edges (source === target)", () => {
    // A self-loop must only appear once in the adjacency list or
    // expansion UI double-counts the degree. This was the bug the
    // explicit `source === target` guard exists to prevent.
    const ir: OntologyIR = {
      id: "ont-loop",
      name: "loop",
      description: { default: "" },
      version: { number: 1 },
      node_types: [
        { id: "n-page", label: "Page", description: { default: "" }, properties: [] },
      ],
      edge_types: [
        {
          id: "e-links-to",
          label: "LINKS_TO",
          description: { default: "" },
          source_node_id: "n-page",
          target_node_id: "n-page",
          properties: [],
          cardinality: "many_to_many",
        },
      ],
    };
    const idx = buildOntologyIndex(ir);
    const pageEdges = idx.edgesByNodeId.get("n-page") ?? [];
    expect(pageEdges).toHaveLength(1);
    expect(pageEdges[0].id).toBe("e-links-to");
  });

  it("returns undefined for nodes with no incident edges", () => {
    // A degree-0 node doesn't get an entry — callers default to an
    // empty list themselves. Leaving the entry absent keeps the map
    // compact for ontologies with many isolated nodes.
    const ir: OntologyIR = {
      id: "ont-solo",
      name: "solo",
      description: { default: "" },
      version: { number: 1 },
      node_types: [
        { id: "n-solo", label: "Solo", description: { default: "" }, properties: [] },
      ],
      edge_types: [],
    };
    const idx = buildOntologyIndex(ir);
    expect(idx.edgesByNodeId.get("n-solo")).toBeUndefined();
  });
});
