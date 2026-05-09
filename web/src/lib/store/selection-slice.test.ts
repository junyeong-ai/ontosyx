import { describe, it, expect, beforeEach } from "vitest";
import { createStore } from "zustand";
import type { AppStore } from "./types";
import { createOntologySlice } from "./ontology-slice";
import { createChatSlice } from "./chat-slice";
import { createChromeSlice } from "./chrome-slice";
import {
  createSelectionSlice,
  selectionContains,
  selectionContainsId,
  selectionOfKind,
  selectionPrimary,
} from "./selection-slice";
import { createDashboardSlice } from "./dashboard-slice";
import { createOntologyDraftSlice } from "./ontology-draft-slice";
import { createVerificationSlice } from "./verification-slice";
import { createNotificationSlice } from "./notification-slice";

function createTestStore() {
  return createStore<AppStore>()((...a) => ({
    ...createOntologySlice(...a),
    ...createChatSlice(...a),
    ...createOntologyDraftSlice(...a),
    ...createChromeSlice(...a),
    ...createSelectionSlice(...a),
    ...createDashboardSlice(...a),
    ...createVerificationSlice(...a),
    ...createNotificationSlice(...a),
  }));
}

describe("SelectionSlice", () => {
  let store: ReturnType<typeof createTestStore>;

  beforeEach(() => {
    store = createTestStore();
  });

  it("defaults to an empty selection", () => {
    expect(store.getState().selection.refs).toEqual([]);
  });

  it("selectOne replaces the selection with a single ref", () => {
    store.getState().selectOne({ kind: "node", id: "n1" });
    expect(store.getState().selection.refs).toEqual([
      { kind: "node", id: "n1" },
    ]);
  });

  it("selectOne with null clears the selection", () => {
    store.getState().selectOne({ kind: "node", id: "n1" });
    store.getState().selectOne(null);
    expect(store.getState().selection.refs).toEqual([]);
  });

  it("selectOne replaces a previous multi-selection", () => {
    store
      .getState()
      .selectMany([
        { kind: "node", id: "n1" },
        { kind: "node", id: "n2" },
      ]);
    store.getState().selectOne({ kind: "edge", id: "e1" });
    expect(store.getState().selection.refs).toEqual([
      { kind: "edge", id: "e1" },
    ]);
  });

  it("toggleSelection appends a new ref and removes a present ref", () => {
    const ref = { kind: "node" as const, id: "n1" };
    store.getState().toggleSelection(ref);
    expect(store.getState().selection.refs).toEqual([ref]);
    store.getState().toggleSelection(ref);
    expect(store.getState().selection.refs).toEqual([]);
  });

  it("toggleSelection on a different kind+id keeps both", () => {
    store.getState().toggleSelection({ kind: "node", id: "n1" });
    store.getState().toggleSelection({ kind: "edge", id: "n1" });
    expect(store.getState().selection.refs).toEqual([
      { kind: "node", id: "n1" },
      { kind: "edge", id: "n1" },
    ]);
  });

  it("extendSelection skips refs already present", () => {
    store.getState().toggleSelection({ kind: "node", id: "n1" });
    store
      .getState()
      .extendSelection([
        { kind: "node", id: "n1" },
        { kind: "node", id: "n2" },
      ]);
    expect(store.getState().selection.refs).toEqual([
      { kind: "node", id: "n1" },
      { kind: "node", id: "n2" },
    ]);
  });

  it("selectMany dedupes the input list", () => {
    store.getState().selectMany([
      { kind: "node", id: "n1" },
      { kind: "node", id: "n1" },
      { kind: "node", id: "n2" },
    ]);
    expect(store.getState().selection.refs).toEqual([
      { kind: "node", id: "n1" },
      { kind: "node", id: "n2" },
    ]);
  });

  it("clearSelection empties refs", () => {
    store.getState().selectOne({ kind: "node", id: "n1" });
    store.getState().clearSelection();
    expect(store.getState().selection.refs).toEqual([]);
  });
});

describe("selection helpers", () => {
  it("selectionPrimary returns the most recently added ref", () => {
    const sel = {
      refs: [
        { kind: "node" as const, id: "n1" },
        { kind: "edge" as const, id: "e1" },
      ],
    };
    expect(selectionPrimary(sel)).toEqual({ kind: "edge", id: "e1" });
  });

  it("selectionPrimary returns null when empty", () => {
    expect(selectionPrimary({ refs: [] })).toBeNull();
  });

  it("selectionContains is kind-aware", () => {
    const sel = { refs: [{ kind: "node" as const, id: "x" }] };
    expect(selectionContains(sel, { kind: "node", id: "x" })).toBe(true);
    expect(selectionContains(sel, { kind: "edge", id: "x" })).toBe(false);
    expect(selectionContains(sel, { kind: "node", id: "y" })).toBe(false);
  });

  it("selectionContainsId is the same check via flat args", () => {
    const sel = { refs: [{ kind: "node" as const, id: "x" }] };
    expect(selectionContainsId(sel, "node", "x")).toBe(true);
    expect(selectionContainsId(sel, "edge", "x")).toBe(false);
  });

  it("selectionOfKind filters refs by kind", () => {
    const sel = {
      refs: [
        { kind: "node" as const, id: "n1" },
        { kind: "edge" as const, id: "e1" },
        { kind: "node" as const, id: "n2" },
      ],
    };
    expect(selectionOfKind(sel, "node")).toEqual([
      { kind: "node", id: "n1" },
      { kind: "node", id: "n2" },
    ]);
    expect(selectionOfKind(sel, "edge")).toEqual([{ kind: "edge", id: "e1" }]);
    expect(selectionOfKind(sel, "widget")).toEqual([]);
  });
});
