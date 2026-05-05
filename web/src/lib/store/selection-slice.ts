import type { StateCreator } from "zustand";
import type {
  AppStore,
  Selection,
  SelectionKind,
  SelectionRef,
  SelectionSlice,
} from "./types";

// ---------------------------------------------------------------------------
// Pure helpers — kept here so reducers and selectors share one definition.
// ---------------------------------------------------------------------------

export function refKey(ref: SelectionRef): string {
  return `${ref.kind}:${ref.id}`;
}

export function selectionPrimary(s: Selection): SelectionRef | null {
  return s.refs.length === 0 ? null : s.refs[s.refs.length - 1];
}

export function selectionContains(
  s: Selection,
  ref: SelectionRef,
): boolean {
  return s.refs.some((r) => r.kind === ref.kind && r.id === ref.id);
}

export function selectionContainsId(
  s: Selection,
  kind: SelectionKind,
  id: string,
): boolean {
  return s.refs.some((r) => r.kind === kind && r.id === id);
}

/** Filter the selection to refs of a given kind. Returns a fresh array. */
export function selectionOfKind(
  s: Selection,
  kind: SelectionKind,
): SelectionRef[] {
  return s.refs.filter((r) => r.kind === kind);
}

// ---------------------------------------------------------------------------
// Slice
// ---------------------------------------------------------------------------

const EMPTY: Selection = { refs: [] };

function dedupeAppend(
  base: readonly SelectionRef[],
  incoming: readonly SelectionRef[],
): SelectionRef[] {
  const seen = new Set(base.map(refKey));
  const out = [...base];
  for (const ref of incoming) {
    const key = refKey(ref);
    if (seen.has(key)) continue;
    seen.add(key);
    out.push(ref);
  }
  return out;
}

export const createSelectionSlice: StateCreator<
  AppStore,
  [],
  [],
  SelectionSlice
> = (set) => ({
  selection: EMPTY,
  selectOne: (ref) =>
    set({ selection: ref ? { refs: [ref] } : EMPTY }),
  toggleSelection: (ref) =>
    set((state) => {
      const key = refKey(ref);
      const idx = state.selection.refs.findIndex((r) => refKey(r) === key);
      if (idx === -1) {
        return { selection: { refs: [...state.selection.refs, ref] } };
      }
      const next = state.selection.refs.filter((r) => refKey(r) !== key);
      return { selection: { refs: next } };
    }),
  extendSelection: (refs) =>
    set((state) => ({
      selection: { refs: dedupeAppend(state.selection.refs, refs) },
    })),
  selectMany: (refs) => {
    // Dedupe inputs while preserving order — a caller that passes
    // duplicates (e.g. lasso of a node + edge that share a label)
    // shouldn't see the inspector flip-flop on which is primary.
    const seen = new Set<string>();
    const next: SelectionRef[] = [];
    for (const ref of refs) {
      const key = refKey(ref);
      if (seen.has(key)) continue;
      seen.add(key);
      next.push(ref);
    }
    set({ selection: { refs: next } });
  },
  clearSelection: () => set({ selection: EMPTY }),
  neighborhoodFocus: null,
  setNeighborhoodFocus: (focus) => set({ neighborhoodFocus: focus }),
});
