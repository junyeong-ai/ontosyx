import { describe, expect, it } from "vitest";

import en from "../../messages/en.json";
import ko from "../../messages/ko.json";

/**
 * Walk a nested message dictionary and yield the dotted path of
 * every leaf string. The catalogue convention: leaves are strings
 * (translatable templates), interior nodes are namespacing
 * objects. Arrays are not part of the convention — flagged as
 * leaves so a misshapen entry surfaces in the parity diff.
 */
function flattenLeafKeys(obj: unknown, prefix = ""): string[] {
  if (obj === null) return [prefix];
  if (typeof obj !== "object") return [prefix];
  if (Array.isArray(obj)) return [prefix];
  const out: string[] = [];
  for (const [key, value] of Object.entries(obj as Record<string, unknown>)) {
    const path = prefix ? `${prefix}.${key}` : key;
    if (
      value !== null &&
      typeof value === "object" &&
      !Array.isArray(value)
    ) {
      out.push(...flattenLeafKeys(value, path));
    } else {
      out.push(path);
    }
  }
  return out.sort();
}

/**
 * The locale catalogues are siblings: every key authored in one
 * must appear in the other, otherwise a Korean user silently
 * falls back to the English template (or vice-versa) and the
 * locale axis becomes meaningless. This test is the contract gate
 * that keeps the two trees identical.
 */
describe("messages catalogue parity", () => {
  it("en and ko have identical leaf-key sets", () => {
    const enKeys = new Set(flattenLeafKeys(en));
    const koKeys = new Set(flattenLeafKeys(ko));

    const onlyInEn = [...enKeys].filter((k) => !koKeys.has(k)).sort();
    const onlyInKo = [...koKeys].filter((k) => !enKeys.has(k)).sort();

    expect(
      { onlyInEn, onlyInKo },
      "catalogue trees diverged — every key must exist in both en.json and ko.json",
    ).toEqual({ onlyInEn: [], onlyInKo: [] });
  });
});
