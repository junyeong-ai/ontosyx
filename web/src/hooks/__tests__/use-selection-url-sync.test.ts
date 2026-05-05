import { describe, expect, it } from "vitest";
import {
  decodeSelectionParam,
  encodeSelectionParam,
} from "../use-selection-url-sync";

describe("encodeSelectionParam", () => {
  it("renders kind shorthand and joins with commas", () => {
    expect(
      encodeSelectionParam([
        { kind: "node", id: "abc" },
        { kind: "edge", id: "xyz" },
        { kind: "widget", id: "w1" },
      ]),
    ).toBe("n:abc,e:xyz,w:w1");
  });

  it("returns empty string for an empty selection", () => {
    expect(encodeSelectionParam([])).toBe("");
  });
});

describe("decodeSelectionParam", () => {
  it("round-trips with encodeSelectionParam", () => {
    const refs = [
      { kind: "node" as const, id: "abc" },
      { kind: "edge" as const, id: "xyz" },
    ];
    expect(decodeSelectionParam(encodeSelectionParam(refs))).toEqual(refs);
  });

  it("returns empty array for null / empty input", () => {
    expect(decodeSelectionParam(null)).toEqual([]);
    expect(decodeSelectionParam("")).toEqual([]);
  });

  it("dedupes repeated refs in the URL", () => {
    expect(decodeSelectionParam("n:a,n:a,n:b")).toEqual([
      { kind: "node", id: "a" },
      { kind: "node", id: "b" },
    ]);
  });

  it("ignores malformed segments without aborting the rest", () => {
    expect(decodeSelectionParam("n:a,bogus,e:b,:nokind")).toEqual([
      { kind: "node", id: "a" },
      { kind: "edge", id: "b" },
    ]);
  });

  it("ignores unknown kind shortcodes", () => {
    expect(decodeSelectionParam("z:a,n:b")).toEqual([
      { kind: "node", id: "b" },
    ]);
  });

  it("preserves order — first entry wins on dedup", () => {
    expect(decodeSelectionParam("n:b,n:a,n:b")).toEqual([
      { kind: "node", id: "b" },
      { kind: "node", id: "a" },
    ]);
  });
});
