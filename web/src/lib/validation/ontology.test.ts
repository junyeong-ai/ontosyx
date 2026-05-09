import { describe, expect, it } from "vitest";

import { OntologyIRSchema, PropertyValueSchema } from "./ontology";

describe("PropertyValueSchema", () => {
  it("accepts recursive tagged list and map values", () => {
    expect(
      PropertyValueSchema.parse({
        type: "map",
        value: {
          name: { type: "string", value: "Alice" },
          scores: {
            type: "list",
            value: [{ type: "int", value: 10 }],
          },
        },
      }),
    ).toEqual({
      type: "map",
      value: {
        name: { type: "string", value: "Alice" },
        scores: {
          type: "list",
          value: [{ type: "int", value: 10 }],
        },
      },
    });
  });

  it("rejects loose untagged default values", () => {
    expect(() => PropertyValueSchema.parse("Alice")).toThrow();
    expect(() => PropertyValueSchema.parse({ type: "list" })).toThrow();
    expect(() =>
      PropertyValueSchema.parse({ type: "string", value: "Alice", extra: true }),
    ).toThrow();
  });
});

describe("OntologyIRSchema", () => {
  it("preserves advanced ontology collections after core validation", () => {
    const parsed = OntologyIRSchema.parse({
      id: "ont-1",
      name: "Sales",
      description: { default: "Sales ontology" },
      version: { number: 3 },
      node_types: [],
      edge_types: [],
      object_mappings: [
        {
          id: "customer_object",
          node_type_id: "Customer",
          source: { table: "customers" },
          property_mappings: [],
        },
      ],
      link_mappings: [
        {
          id: "customer_order_link",
          edge_type_id: "PLACED",
          source: { table: "orders" },
          property_mappings: [],
        },
      ],
      glossary: [{ id: "term_customer", term: { default: "Customer" } }],
      rules: [{ id: "rule_customer_key", kind: "custom" }],
    });

    expect(parsed.object_mappings).toHaveLength(1);
    expect(parsed.link_mappings).toHaveLength(1);
    expect(parsed.glossary).toHaveLength(1);
    expect(parsed.rules).toHaveLength(1);
  });
});
