import { describe, expect, it } from "vitest";

import { snapshotEqual } from "./snapshot-equal";

describe("snapshotEqual", () => {
  it("returns true for identical primitives", () => {
    expect(snapshotEqual("a", "a")).toBe(true);
    expect(snapshotEqual(1, 1)).toBe(true);
    expect(snapshotEqual(true, true)).toBe(true);
    expect(snapshotEqual(null, null)).toBe(true);
    expect(snapshotEqual(undefined, undefined)).toBe(true);
  });

  it("returns false for distinct primitives", () => {
    expect(snapshotEqual("a", "b")).toBe(false);
    expect(snapshotEqual(1, 2)).toBe(false);
    expect(snapshotEqual(true, false)).toBe(false);
    expect(snapshotEqual(null, undefined)).toBe(false);
    expect(snapshotEqual(0, false)).toBe(false);
  });

  it("compares arrays element-by-element", () => {
    expect(snapshotEqual([1, 2, 3], [1, 2, 3])).toBe(true);
    expect(snapshotEqual([1, 2], [1, 2, 3])).toBe(false);
    expect(snapshotEqual([1, 2, 3], [3, 2, 1])).toBe(false);
    expect(snapshotEqual([], [])).toBe(true);
  });

  it("compares plain objects regardless of key order", () => {
    // The key win over JSON.stringify — V8 guarantees insertion
    // order, so a snapshot rebuilt with slots in a different
    // sequence stringifies differently even when content matches.
    expect(snapshotEqual({ a: 1, b: 2 }, { b: 2, a: 1 })).toBe(true);
    expect(snapshotEqual({ a: 1 }, { a: 1, b: undefined })).toBe(false);
  });

  it("distinguishes `undefined` slot from absent slot", () => {
    // JSON.stringify drops `undefined` values, conflating the
    // two. A real form often distinguishes "user cleared the
    // field" (undefined) from "field didn't exist before".
    expect(snapshotEqual({ a: undefined }, {})).toBe(false);
  });

  it("recurses into nested objects and arrays", () => {
    expect(
      snapshotEqual(
        { a: [1, { b: "x" }], c: { d: [true, null] } },
        { a: [1, { b: "x" }], c: { d: [true, null] } },
      ),
    ).toBe(true);
    expect(
      snapshotEqual(
        { a: [1, { b: "x" }] },
        { a: [1, { b: "y" }] },
      ),
    ).toBe(false);
  });

  it("treats arrays and objects as distinct types", () => {
    expect(snapshotEqual([], {})).toBe(false);
  });

  it("returns true when given two references to the same value", () => {
    const obj = { a: 1, b: [2, 3] };
    expect(snapshotEqual(obj, obj)).toBe(true);
  });

  it("treats NaN as equal to itself (Object.is semantics)", () => {
    expect(snapshotEqual(NaN, NaN)).toBe(true);
  });
});
