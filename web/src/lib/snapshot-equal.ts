// Structural equality for form-snapshot shapes — plain
// objects/arrays of primitives. Replaces
// `JSON.stringify(a) !== JSON.stringify(b)` dirty-checks where:
//
// - object key order differs (engines aren't guaranteed
//   identical, and a re-build of the snapshot through the same
//   slot list with a different `useMemo` dep order can shuffle
//   keys silently).
// - a slot can hold `undefined` vs the same key being absent
//   (stringify drops `undefined`, so `{a: undefined}` and `{}`
//   compare equal even though the React state legitimately
//   distinguishes them).
//
// Scope is intentionally narrow: SaveBar / `useDraftPersistence`
// snapshots only ever hold strings, numbers, booleans, null,
// arrays of those, and shallow objects with the same. No Map /
// Set / Date / function / class instance support — promote to a
// vetted dep (dequal) on the day a snapshot needs them.

export function snapshotEqual(a: unknown, b: unknown): boolean {
  if (Object.is(a, b)) return true;
  if (a === null || b === null) return false;
  if (typeof a !== typeof b) return false;
  if (typeof a !== "object") return false;

  if (Array.isArray(a)) {
    if (!Array.isArray(b) || a.length !== b.length) return false;
    for (let i = 0; i < a.length; i++) {
      if (!snapshotEqual(a[i], b[i])) return false;
    }
    return true;
  }
  if (Array.isArray(b)) return false;

  const ao = a as Record<string, unknown>;
  const bo = b as Record<string, unknown>;
  const ak = Object.keys(ao);
  const bk = Object.keys(bo);
  if (ak.length !== bk.length) return false;
  for (const k of ak) {
    if (!Object.prototype.hasOwnProperty.call(bo, k)) return false;
    if (!snapshotEqual(ao[k], bo[k])) return false;
  }
  return true;
}
