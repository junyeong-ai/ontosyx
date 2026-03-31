import { describe, it, expect } from "vitest";
import { unwrapPropertyValue, extractNodeProperties, normalizeQueryResult } from "./normalization";

describe("unwrapPropertyValue", () => {
  it("passes through primitives", () => {
    expect(unwrapPropertyValue(42)).toBe(42);
    expect(unwrapPropertyValue("hello")).toBe("hello");
    expect(unwrapPropertyValue(true)).toBe(true);
  });

  it("returns null for null/undefined", () => {
    expect(unwrapPropertyValue(null)).toBeNull();
    expect(unwrapPropertyValue(undefined)).toBeNull();
  });

  it("unwraps scalar PropertyValue", () => {
    expect(unwrapPropertyValue({ type: "int", value: 42 })).toBe(42);
    expect(unwrapPropertyValue({ type: "string", value: "test" })).toBe("test");
    expect(unwrapPropertyValue({ type: "float", value: 3.14 })).toBe(3.14);
  });

  it("unwraps null PropertyValue", () => {
    expect(unwrapPropertyValue({ type: "null" })).toBeNull();
  });

  it("unwraps list PropertyValue recursively", () => {
    const input = {
      type: "list",
      value: [
        { type: "int", value: 1 },
        { type: "int", value: 2 },
        { type: "string", value: "three" },
      ],
    };
    expect(unwrapPropertyValue(input)).toEqual([1, 2, "three"]);
  });

  it("unwraps nested list", () => {
    const input = {
      type: "list",
      value: [
        { type: "list", value: [{ type: "int", value: 1 }] },
      ],
    };
    expect(unwrapPropertyValue(input)).toEqual([[1]]);
  });

  it("unwraps map PropertyValue", () => {
    const input = {
      type: "map",
      value: {
        name: { type: "string", value: "Alice" },
        age: { type: "int", value: 30 },
      },
    };
    expect(unwrapPropertyValue(input)).toEqual({ name: "Alice", age: 30 });
  });

  it("passes through plain objects without type field", () => {
    const obj = { name: "test", count: 5 };
    expect(unwrapPropertyValue(obj)).toEqual(obj);
  });

  it("unwraps arrays recursively", () => {
    const input = [{ type: "int", value: 1 }, { type: "int", value: 2 }];
    expect(unwrapPropertyValue(input)).toEqual([1, 2]);
  });
});

describe("extractNodeProperties", () => {
  it("extracts from structured node (properties sub-object)", () => {
    const node = {
      labels: ["Person"],
      properties: { name: "Alice", age: 30 },
      id: 123,
      keys: ["name", "age"],
    };
    expect(extractNodeProperties(node)).toEqual({ name: "Alice", age: 30 });
  });

  it("extracts from flat node (strips metadata)", () => {
    const node = {
      labels: ["Product"],
      id: 456,
      element_id: "4:abc:456",
      keys: ["name", "price"],
      name: "Widget",
      price: 9.99,
    };
    expect(extractNodeProperties(node)).toEqual({ name: "Widget", price: 9.99 });
  });

  it("returns null for non-node objects", () => {
    expect(extractNodeProperties({ name: "test" })).toBeNull();
    expect(extractNodeProperties({ count: 5 })).toBeNull();
  });

  it("returns null for flat node with only metadata", () => {
    const node = { labels: ["Empty"], id: 1, element_id: "x", keys: [] };
    expect(extractNodeProperties(node)).toBeNull();
  });
});

describe("normalizeQueryResult", () => {
  it("returns undefined for invalid input", () => {
    expect(normalizeQueryResult(null)).toBeUndefined();
    expect(normalizeQueryResult({})).toBeUndefined();
    expect(normalizeQueryResult({ columns: [] })).toBeUndefined();
    expect(normalizeQueryResult({ rows: [] })).toBeUndefined();
  });

  it("normalizes array-format rows", () => {
    const raw = {
      columns: ["name", "age"],
      rows: [
        [{ type: "string", value: "Alice" }, { type: "int", value: 30 }],
        [{ type: "string", value: "Bob" }, { type: "int", value: 25 }],
      ],
    };
    const result = normalizeQueryResult(raw)!;
    expect(result.columns).toEqual(["name", "age"]);
    expect(result.rows).toEqual([
      { name: "Alice", age: 30 },
      { name: "Bob", age: 25 },
    ]);
  });

  it("normalizes object-format rows", () => {
    const raw = {
      columns: ["name"],
      rows: [{ name: { type: "string", value: "Alice" } }],
    };
    const result = normalizeQueryResult(raw)!;
    expect(result.rows).toEqual([{ name: "Alice" }]);
  });

  it("flattens single-column node results", () => {
    const raw = {
      columns: ["p"],
      rows: [
        [{ name: "Widget", price: 9.99 }],
        [{ name: "Gadget", price: 19.99 }],
      ],
    };
    const result = normalizeQueryResult(raw)!;
    expect(result.columns).toContain("name");
    expect(result.columns).toContain("price");
    expect(result.rows[0]).toEqual({ name: "Widget", price: 9.99 });
  });

  it("flattens structured Neo4j nodes", () => {
    const raw = {
      columns: ["n"],
      rows: [
        [{ labels: ["Person"], properties: { name: "Alice", age: 30 }, id: 1 }],
      ],
    };
    const result = normalizeQueryResult(raw)!;
    expect(result.columns).toContain("name");
    expect(result.columns).toContain("age");
    expect(result.rows[0]).toEqual({ name: "Alice", age: 30 });
  });

  it("preserves metadata", () => {
    const raw = {
      columns: ["x"],
      rows: [[{ type: "int", value: 1 }]],
      metadata: { execution_time_ms: 42 },
    };
    const result = normalizeQueryResult(raw)!;
    expect(result.metadata).toEqual({ execution_time_ms: 42 });
  });
});
