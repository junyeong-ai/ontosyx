import { describe, it, expect } from "vitest";
import { createStore } from "zustand";
import type { AppStore } from "./types";
import type { OntologyIR } from "@/types/api";
import { createOntologySlice } from "./ontology-slice";
import { createChatSlice } from "./chat-slice";
import { createChromeSlice } from "./chrome-slice";
import { createSelectionSlice } from "./selection-slice";
import { createDashboardSlice } from "./dashboard-slice";
import { createProjectSlice } from "./project-slice";
import { createVerificationSlice } from "./verification-slice";
import {
  selectStateHasOntology,
  selectStateHasUnsavedEdits,
  selectStateSelectedNodeId,
  selectStateSelectedEdgeId,
  selectStateSelectedWidgetId,
  selectStateCanChat,
} from "./selectors";

function createTestStore() {
  return createStore<AppStore>()((...a) => ({
    ...createOntologySlice(...a),
    ...createChatSlice(...a),
    ...createProjectSlice(...a),
    ...createChromeSlice(...a),
    ...createSelectionSlice(...a),
    ...createDashboardSlice(...a),
    ...createVerificationSlice(...a),
  }));
}

const MINIMAL_ONTOLOGY: OntologyIR = {
  id: "test",
  name: "Test",
description: { default: "" },
  version: { number: 1 },
  node_types: [{ id: "n1", label: "Person",
description: { default: "" }, properties: [] }],
  edge_types: [],
};

describe("Selectors", () => {
  it("selectStateHasOntology returns false when null", () => {
    const store = createTestStore();
    expect(selectStateHasOntology(store.getState())).toBe(false);
  });

  it("selectStateHasOntology returns true when set", () => {
    const store = createTestStore();
    store.getState().loadStandaloneOntology(MINIMAL_ONTOLOGY);
    expect(selectStateHasOntology(store.getState())).toBe(true);
  });

  it("selectStateHasUnsavedEdits reflects command stack", () => {
    const store = createTestStore();
    expect(selectStateHasUnsavedEdits(store.getState())).toBe(false);

    store.getState().loadStandaloneOntology(MINIMAL_ONTOLOGY);
    store.getState().applyCommand({ op: "add_node", id: "n2", label: "Product" });
    expect(selectStateHasUnsavedEdits(store.getState())).toBe(true);
  });

  it("selectStateSelectedNodeId extracts from selection", () => {
    const store = createTestStore();
    expect(selectStateSelectedNodeId(store.getState())).toBeNull();

    store.getState().selectOne({ kind: "node", id: "n1" });
    expect(selectStateSelectedNodeId(store.getState())).toBe("n1");

    store.getState().selectOne({ kind: "edge", id: "e1" });
    expect(selectStateSelectedNodeId(store.getState())).toBeNull();
  });

  it("selectStateSelectedEdgeId extracts from selection", () => {
    const store = createTestStore();
    store.getState().selectOne({ kind: "edge", id: "e1" });
    expect(selectStateSelectedEdgeId(store.getState())).toBe("e1");

    store.getState().clearSelection();
    expect(selectStateSelectedEdgeId(store.getState())).toBeNull();
  });

  it("selectStateSelectedWidgetId extracts from selection", () => {
    const store = createTestStore();
    store.getState().selectOne({ kind: "widget", id: "w1" });
    expect(selectStateSelectedWidgetId(store.getState())).toBe("w1");
  });

  it("selectStateCanChat mirrors selectStateHasOntology", () => {
    const store = createTestStore();
    expect(selectStateCanChat(store.getState())).toBe(false);

    store.getState().loadStandaloneOntology(MINIMAL_ONTOLOGY);
    expect(selectStateCanChat(store.getState())).toBe(true);
  });
});
