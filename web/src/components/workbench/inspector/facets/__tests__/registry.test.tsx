import { beforeEach, describe, expect, it } from "vitest";

import {
  _resetInspectorFacetRegistryForTests,
  inspectorFacetById,
  listInspectorFacets,
  registerInspectorFacet,
  unregisterInspectorFacet,
  visibleInspectorFacets,
  type InspectorFacetContext,
} from "../registry";
import type {
  EdgeTypeDef,
  NodeTypeDef,
  OntologyIR,
} from "@/types/api";

const ONTOLOGY: OntologyIR = {
  id: "ont",
  name: "Test",
  description: { default: "" },
  version: { number: 1 },
  node_types: [],
  edge_types: [],
};

const NODE_WITH_LINEAGE: NodeTypeDef = {
  id: "n1",
  label: "Person",
  description: { default: "" },
  properties: [],
  source_lineage: { table: "people" },
};

const NODE_WITHOUT_LINEAGE: NodeTypeDef = {
  id: "n2",
  label: "Synthetic",
  description: { default: "" },
  properties: [],
};

const EDGE: EdgeTypeDef = {
  id: "e1",
  label: "knows",
  description: { default: "" },
  properties: [],
  source_node_id: "n1",
  target_node_id: "n2",
};

function nodeCtx(node: NodeTypeDef): InspectorFacetContext {
  return {
    ontology: ONTOLOGY,
    kind: "node",
    entityRef: { kind: "node_type", id: node.id },
    entity: node,
    node,
    edge: null,
    gaps: [],
    inboundCount: 0,
    outboundCount: 0,
  };
}

function edgeCtx(edge: EdgeTypeDef): InspectorFacetContext {
  return {
    ontology: ONTOLOGY,
    kind: "edge",
    entityRef: { kind: "edge_type", id: edge.id },
    entity: edge,
    node: null,
    edge,
    gaps: [],
    inboundCount: 0,
    outboundCount: 0,
  };
}

describe("INSPECTOR_FACETS", () => {
  it("exposes a non-empty registry of facets", () => {
    expect(listInspectorFacets().length).toBeGreaterThan(0);
  });

  it("entries are unique by id", () => {
    const ids = listInspectorFacets().map((f) => f.id);
    expect(new Set(ids).size).toBe(ids.length);
  });
});

describe("registerInspectorFacet / unregisterInspectorFacet", () => {
  beforeEach(() => {
    _resetInspectorFacetRegistryForTests();
  });

  it("appends a new facet at the end by default", () => {
    registerInspectorFacet({
      id: "permissions",
      labelKey: "permissions",
      accept: () => true,
      render: () => null,
    });
    const ids = listInspectorFacets().map((f) => f.id);
    expect(ids[ids.length - 1]).toBe("permissions");
  });

  it("`before` inserts ahead of the named facet", () => {
    registerInspectorFacet(
      {
        id: "audit",
        labelKey: "audit",
        accept: () => true,
        render: () => null,
      },
      { before: "lineage" },
    );
    const ids = listInspectorFacets().map((f) => f.id);
    expect(ids.indexOf("audit")).toBe(ids.indexOf("lineage") - 1);
  });

  it("`after` inserts behind the named facet", () => {
    registerInspectorFacet(
      {
        id: "audit",
        labelKey: "audit",
        accept: () => true,
        render: () => null,
      },
      { after: "definition" },
    );
    const ids = listInspectorFacets().map((f) => f.id);
    expect(ids.indexOf("audit")).toBe(ids.indexOf("definition") + 1);
  });

  it("re-registering an existing id replaces the entry in place", () => {
    const before = listInspectorFacets().map((f) => f.id);
    registerInspectorFacet({
      id: "lineage",
      labelKey: "lineage",
      accept: () => false, // override
      render: () => null,
    });
    const after = listInspectorFacets().map((f) => f.id);
    expect(after).toEqual(before);
    expect(inspectorFacetById("lineage")?.accept({} as InspectorFacetContext)).toBe(
      false,
    );
  });

  it("unregister removes the facet and is idempotent", () => {
    registerInspectorFacet({
      id: "permissions",
      labelKey: "permissions",
      accept: () => true,
      render: () => null,
    });
    unregisterInspectorFacet("permissions");
    expect(inspectorFacetById("permissions")).toBeUndefined();
    // Second call doesn't throw on unknown id.
    expect(() => unregisterInspectorFacet("permissions")).not.toThrow();
  });
});

describe("visibleInspectorFacets", () => {
  it("a node with source lineage shows the sample tab", () => {
    const visible = visibleInspectorFacets(nodeCtx(NODE_WITH_LINEAGE));
    expect(visible.map((f) => f.id)).toContain("sample");
  });

  it("a node without source lineage hides the sample tab", () => {
    const visible = visibleInspectorFacets(nodeCtx(NODE_WITHOUT_LINEAGE));
    expect(visible.map((f) => f.id)).not.toContain("sample");
  });

  it("edges never show the sample tab", () => {
    const visible = visibleInspectorFacets(edgeCtx(EDGE));
    expect(visible.map((f) => f.id)).not.toContain("sample");
  });

  it("preserves declaration order", () => {
    const visible = visibleInspectorFacets(nodeCtx(NODE_WITH_LINEAGE));
    expect(visible.map((f) => f.id)).toEqual([
      "definition",
      "sample",
      "lineage",
      "rules",
      "quality",
      "changelog",
    ]);
  });
});

describe("facet badges", () => {
  it("the lineage facet sums inbound + outbound", () => {
    const facet = inspectorFacetById("lineage")!;
    expect(
      facet.badge?.({ ...nodeCtx(NODE_WITH_LINEAGE), inboundCount: 0, outboundCount: 0 }),
    ).toBeUndefined();
    expect(
      facet.badge?.({ ...nodeCtx(NODE_WITH_LINEAGE), inboundCount: 2, outboundCount: 3 }),
    ).toBe(5);
  });

  it("the quality facet reflects gap count", () => {
    const facet = inspectorFacetById("quality")!;
    expect(facet.badge?.(nodeCtx(NODE_WITH_LINEAGE))).toBeUndefined();
    const ctx = {
      ...nodeCtx(NODE_WITH_LINEAGE),
      gaps: [
        { severity: "high" } as never,
        { severity: "medium" } as never,
      ],
    };
    expect(facet.badge?.(ctx)).toBe(2);
  });
});

describe("inspectorFacetById", () => {
  it("looks facets up by id", () => {
    expect(inspectorFacetById("definition")?.id).toBe("definition");
    expect(inspectorFacetById("changelog")?.id).toBe("changelog");
    expect(inspectorFacetById("nonexistent")).toBeUndefined();
  });
});
