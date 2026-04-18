import { describe, it, expect } from "vitest";

import { parseFilterValue, stringifyFilterValue } from "./filter-value";

describe("parseFilterValue", () => {
  it("parses integers as number", () => {
    expect(parseFilterValue("42")).toBe(42);
    expect(parseFilterValue("-7")).toBe(-7);
  });

  it("parses floats as number", () => {
    expect(parseFilterValue("1.5")).toBe(1.5);
    expect(parseFilterValue("-0.25")).toBe(-0.25);
  });

  it("parses booleans", () => {
    expect(parseFilterValue("true")).toBe(true);
    expect(parseFilterValue("false")).toBe(false);
  });

  it("strips single quotes", () => {
    expect(parseFilterValue("'hello'")).toBe("hello");
  });

  it("strips double quotes", () => {
    expect(parseFilterValue('"hello"')).toBe("hello");
  });

  it("returns raw string for un-recognised inputs", () => {
    expect(parseFilterValue("hello")).toBe("hello");
    expect(parseFilterValue("not-a-number")).toBe("not-a-number");
  });

  it("preserves mixed-quoted strings as-is", () => {
    // Only matched pairs strip; mismatched stays raw.
    expect(parseFilterValue("'mixed\"")).toBe("'mixed\"");
  });
});

describe("stringifyFilterValue", () => {
  it("returns empty string for null / undefined", () => {
    expect(stringifyFilterValue(null)).toBe("");
    expect(stringifyFilterValue(undefined)).toBe("");
  });

  it("passes strings through unchanged", () => {
    expect(stringifyFilterValue("abc")).toBe("abc");
  });

  it("stringifies numbers", () => {
    expect(stringifyFilterValue(42)).toBe("42");
    expect(stringifyFilterValue(1.5)).toBe("1.5");
  });

  it("stringifies booleans", () => {
    expect(stringifyFilterValue(true)).toBe("true");
    expect(stringifyFilterValue(false)).toBe("false");
  });
});

describe("parse ↔ stringify round trip", () => {
  it("number: string → typed → string", () => {
    for (const raw of ["0", "42", "-7", "1.5", "-0.25"]) {
      expect(stringifyFilterValue(parseFilterValue(raw))).toBe(raw);
    }
  });

  it("boolean: string → typed → string", () => {
    for (const raw of ["true", "false"]) {
      expect(stringifyFilterValue(parseFilterValue(raw))).toBe(raw);
    }
  });

  it("quoted string: surrounding quotes do not survive round trip", () => {
    // Documented asymmetry: the UI strips quotes on parse (so the
    // filter compares against the raw string) but stringify does not
    // re-quote. Round-trip is lossy here by design — the stored
    // wire value is the un-quoted string.
    expect(stringifyFilterValue(parseFilterValue("'hello'"))).toBe("hello");
  });

  it("plain string: round trip is stable", () => {
    for (const raw of ["hello", "not-a-number", ""]) {
      expect(stringifyFilterValue(parseFilterValue(raw))).toBe(raw);
    }
  });
});
