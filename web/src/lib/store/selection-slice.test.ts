import { describe, it, expect, beforeEach } from "vitest";
import { createStore } from "zustand";
import type { AppStore } from "./types";
import { createOntologySlice } from "./ontology-slice";
import { createChatSlice } from "./chat-slice";
import { createChromeSlice } from "./chrome-slice";
import { createSelectionSlice } from "./selection-slice";
import { createDashboardSlice } from "./dashboard-slice";
import { createProjectSlice } from "./project-slice";
import { createVerificationSlice } from "./verification-slice";

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

describe("SelectionSlice", () => {
  let store: ReturnType<typeof createTestStore>;

  beforeEach(() => {
    store = createTestStore();
  });

  it("defaults to no selection", () => {
    expect(store.getState().selection).toEqual({ type: "none" });
  });

  it("select node", () => {
    store.getState().select({ type: "node", nodeId: "n1" });
    expect(store.getState().selection).toEqual({ type: "node", nodeId: "n1" });
  });

  it("select edge", () => {
    store.getState().select({ type: "edge", edgeId: "e1" });
    expect(store.getState().selection).toEqual({ type: "edge", edgeId: "e1" });
  });

  it("select widget", () => {
    store.getState().select({ type: "widget", widgetId: "w1" });
    expect(store.getState().selection).toEqual({ type: "widget", widgetId: "w1" });
  });

  it("selecting replaces previous selection", () => {
    store.getState().select({ type: "node", nodeId: "n1" });
    store.getState().select({ type: "edge", edgeId: "e1" });
    expect(store.getState().selection).toEqual({ type: "edge", edgeId: "e1" });
  });

  it("clearSelection resets to none", () => {
    store.getState().select({ type: "node", nodeId: "n1" });
    store.getState().clearSelection();
    expect(store.getState().selection).toEqual({ type: "none" });
  });

  it("neighborhoodFocus defaults to null", () => {
    expect(store.getState().neighborhoodFocus).toBeNull();
  });

  it("setNeighborhoodFocus sets and clears", () => {
    store.getState().setNeighborhoodFocus({ nodeId: "n1", depth: 2 });
    expect(store.getState().neighborhoodFocus).toEqual({ nodeId: "n1", depth: 2 });

    store.getState().setNeighborhoodFocus(null);
    expect(store.getState().neighborhoodFocus).toBeNull();
  });
});
