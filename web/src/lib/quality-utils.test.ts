import { describe, it, expect } from "vitest";

import type { QualityGap, QualityGapRef } from "@/types/api";

import { getGapEntityId } from "./quality-utils";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

function gap(location: QualityGapRef): QualityGap {
  return {
    severity: "medium",
    category: "missing_description",
    location,
    issue: "fixture",
    suggestion: "fixture",
  };
}

// ---------------------------------------------------------------------------
// getGapEntityId — ref_type → canvas anchor
// ---------------------------------------------------------------------------

describe("getGapEntityId — canvas anchor resolution", () => {
  it("resolves a node ref to the containing node", () => {
    const g = gap({ ref_type: "node", node_id: "n-1", label: "Person" });
    expect(getGapEntityId(g)).toEqual({ type: "node", id: "n-1" });
  });

  it("resolves a node_property ref to the owning node", () => {
    // A property gap lights up the owning node on the canvas — the
    // inspector panel drills into the property from there.
    const g = gap({
      ref_type: "node_property",
      node_id: "n-1",
      property_id: "p-email",
      label: "Person",
      property_name: "email",
    });
    expect(getGapEntityId(g)).toEqual({ type: "node", id: "n-1" });
  });

  it("resolves an edge ref to the containing edge", () => {
    const g = gap({ ref_type: "edge", edge_id: "e-1", label: "WORKS_AT" });
    expect(getGapEntityId(g)).toEqual({ type: "edge", id: "e-1" });
  });

  it("resolves an edge_property ref to the owning edge", () => {
    const g = gap({
      ref_type: "edge_property",
      edge_id: "e-1",
      property_id: "p-since",
      label: "WORKS_AT",
      property_name: "since",
    });
    expect(getGapEntityId(g)).toEqual({ type: "edge", id: "e-1" });
  });

  it("returns null for source-only refs (no canvas anchor)", () => {
    // Source-level gaps (unmapped tables / columns / FKs) have no
    // rendered node on the canvas — the UI shows them in a side
    // panel instead, and must not try to select anything.
    for (const loc of [
      { ref_type: "source_table", table: "users" } as const,
      { ref_type: "source_column", table: "users", column: "email" } as const,
      {
        ref_type: "source_foreign_key",
        from_table: "orders",
        from_column: "user_id",
        to_table: "users",
        to_column: "id",
      } as const,
    ]) {
      expect(getGapEntityId(gap(loc))).toBeNull();
    }
  });
});
