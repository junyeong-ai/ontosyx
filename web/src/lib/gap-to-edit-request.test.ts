import { describe, it, expect } from "vitest";
import { gapToEditRequest } from "./gap-to-edit-request";
import type { QualityGap } from "@/types/api";

function makeGap(overrides: Partial<QualityGap>): QualityGap {
  return {
    severity: "medium",
    category: "missing_description",
    location: { ref_type: "node", node_id: "n1", label: "Brand" },
    issue: "test issue",
    suggestion: "test suggestion",
    ...overrides,
  } as QualityGap;
}

describe("gapToEditRequest", () => {
  it("missing_description on node", () => {
    const result = gapToEditRequest(
      makeGap({
        category: "missing_description",
        location: { ref_type: "node", node_id: "n1", label: "Brand" },
      }),
    );
    expect(result).toContain("Brand");
    expect(result.toLowerCase()).toContain("description");
  });

  it("missing_description on property", () => {
    const result = gapToEditRequest(
      makeGap({
        category: "missing_description",
        location: {
          ref_type: "node_property",
          node_id: "n1",
          property_id: "p1",
          label: "Brand",
          property_name: "country",
        },
      }),
    );
    expect(result).toContain("country");
    expect(result).toContain("Brand");
  });

  it("missing_foreign_key_edge", () => {
    const result = gapToEditRequest(
      makeGap({
        category: "missing_foreign_key_edge",
        location: {
          ref_type: "source_foreign_key",
          from_table: "orders",
          from_column: "brand_id",
          to_table: "brands",
          to_column: "id",
        },
      }),
    );
    expect(result).toContain("orders");
    expect(result).toContain("brands");
  });

  it("unmapped_source_column", () => {
    const result = gapToEditRequest(
      makeGap({
        category: "unmapped_source_column",
        location: {
          ref_type: "source_column",
          table: "products",
          column: "weight",
        },
      }),
    );
    expect(result).toContain("weight");
    expect(result).toContain("products");
  });

  it("orphan_node", () => {
    const result = gapToEditRequest(
      makeGap({
        category: "orphan_node",
        location: { ref_type: "node", node_id: "n1", label: "Settings" },
      }),
    );
    expect(result).toContain("Settings");
    expect(result.toLowerCase()).toContain("edge");
  });

  it("fallback for unknown category", () => {
    const result = gapToEditRequest(
      makeGap({
        category: "some_new_category" as QualityGap["category"],
        issue: "Something is wrong with the data",
      }),
    );
    expect(result).toContain("Something is wrong with the data");
  });
});
