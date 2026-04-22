import { describe, expect, it } from "vitest";

import {
  aggregateEdges,
  type CrossRefEdge,
} from "@/components/ontology/cross-ref-flow";

function edge(
  source: CrossRefEdge["source_axis"],
  target: CrossRefEdge["target_axis"],
  edge_kind = "ref",
): CrossRefEdge {
  return {
    source_axis: source,
    source_kind: "x",
    source_id: `${source}-${Math.random()}`,
    edge_kind,
    target_axis: target,
    target_kind: "y",
    target_id: `${target}-${Math.random()}`,
  };
}

describe("aggregateEdges", () => {
  it("returns empty array for empty input", () => {
    expect(aggregateEdges([])).toEqual([]);
  });

  it("groups by (source_axis, target_axis) and counts", () => {
    const edges = [
      edge("topology", "vocabulary"),
      edge("topology", "vocabulary"),
      edge("topology", "registry"),
      edge("vol", "topology"),
    ];
    const buckets = aggregateEdges(edges);
    const keyed = Object.fromEntries(
      buckets.map((b) => [`${b.source}-${b.target}`, b.count]),
    );
    expect(keyed["topology-vocabulary"]).toBe(2);
    expect(keyed["topology-registry"]).toBe(1);
    expect(keyed["vol-topology"]).toBe(1);
  });

  it("preserves distinct direction — a→b and b→a are separate buckets", () => {
    const buckets = aggregateEdges([
      edge("topology", "registry"),
      edge("registry", "topology"),
    ]);
    expect(buckets).toHaveLength(2);
  });

  it("keeps every individual edge inside its bucket for drill-down", () => {
    const a = edge("topology", "vocabulary", "binds_to");
    const b = edge("topology", "vocabulary", "binds_to");
    const buckets = aggregateEdges([a, b]);
    expect(buckets[0].edges).toHaveLength(2);
    expect(buckets[0].edges[0].edge_kind).toBe("binds_to");
  });

  it("allows self-loop buckets (source === target)", () => {
    const buckets = aggregateEdges([
      edge("topology", "topology"),
      edge("topology", "topology"),
    ]);
    expect(buckets).toHaveLength(1);
    expect(buckets[0].source).toBe("topology");
    expect(buckets[0].target).toBe("topology");
    expect(buckets[0].count).toBe(2);
  });
});
