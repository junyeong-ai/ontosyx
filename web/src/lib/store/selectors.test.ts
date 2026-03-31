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
  selectHasOntology,
  selectHasUnsavedEdits,
  selectSelectedNodeId,
  selectSelectedEdgeId,
  selectSelectedWidgetId,
  selectCanChat,
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
  version: 1,
  node_types: [{ id: "n1", label: "Person", properties: [] }],
  edge_types: [],
};

describe("Selectors", () => {
  it("selectHasOntology returns false when null", () => {
    const store = createTestStore();
    expect(selectHasOntology(store.getState())).toBe(false);
  });

  it("selectHasOntology returns true when set", () => {
    const store = createTestStore();
    store.getState().setOntology(MINIMAL_ONTOLOGY);
    expect(selectHasOntology(store.getState())).toBe(true);
  });

  it("selectHasUnsavedEdits reflects command stack", () => {
    const store = createTestStore();
    expect(selectHasUnsavedEdits(store.getState())).toBe(false);

    store.getState().setOntology(MINIMAL_ONTOLOGY);
    store.getState().applyCommand({ op: "add_node", id: "n2", label: "Product" });
    expect(selectHasUnsavedEdits(store.getState())).toBe(true);
  });

  it("selectSelectedNodeId extracts from selection", () => {
    const store = createTestStore();
    expect(selectSelectedNodeId(store.getState())).toBeNull();

    store.getState().select({ type: "node", nodeId: "n1" });
    expect(selectSelectedNodeId(store.getState())).toBe("n1");

    store.getState().select({ type: "edge", edgeId: "e1" });
    expect(selectSelectedNodeId(store.getState())).toBeNull();
  });

  it("selectSelectedEdgeId extracts from selection", () => {
    const store = createTestStore();
    store.getState().select({ type: "edge", edgeId: "e1" });
    expect(selectSelectedEdgeId(store.getState())).toBe("e1");

    store.getState().clearSelection();
    expect(selectSelectedEdgeId(store.getState())).toBeNull();
  });

  it("selectSelectedWidgetId extracts from selection", () => {
    const store = createTestStore();
    store.getState().select({ type: "widget", widgetId: "w1" });
    expect(selectSelectedWidgetId(store.getState())).toBe("w1");
  });

  it("selectCanChat mirrors selectHasOntology", () => {
    const store = createTestStore();
    expect(selectCanChat(store.getState())).toBe(false);

    store.getState().setOntology(MINIMAL_ONTOLOGY);
    expect(selectCanChat(store.getState())).toBe(true);
  });
});
