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

describe("ChromeSlice", () => {
  let store: ReturnType<typeof createTestStore>;

  beforeEach(() => {
    store = createTestStore();
  });

  it("defaults to design workspace mode", () => {
    expect(store.getState().workspaceMode).toBe("design");
  });

  it("setWorkspaceMode changes mode and clears selection", () => {
    store.getState().select({ type: "node", nodeId: "n1" });
    store.getState().setWorkspaceMode("analyze");
    expect(store.getState().workspaceMode).toBe("analyze");
    expect(store.getState().selection).toEqual({ type: "none" });
  });

  it("designBottomTab defaults to workflow", () => {
    expect(store.getState().designBottomTab).toBe("workflow");
  });

  it("setDesignBottomTab changes tab", () => {
    store.getState().setDesignBottomTab("quality");
    expect(store.getState().designBottomTab).toBe("quality");
  });

  it("toggleExplorer flips isExplorerOpen", () => {
    expect(store.getState().isExplorerOpen).toBe(true);
    store.getState().toggleExplorer();
    expect(store.getState().isExplorerOpen).toBe(false);
    store.getState().toggleExplorer();
    expect(store.getState().isExplorerOpen).toBe(true);
  });

  it("toggleInspector flips isInspectorOpen", () => {
    expect(store.getState().isInspectorOpen).toBe(true);
    store.getState().toggleInspector();
    expect(store.getState().isInspectorOpen).toBe(false);
  });

  it("toggleBottomPanel flips isBottomPanelOpen", () => {
    expect(store.getState().isBottomPanelOpen).toBe(true);
    store.getState().toggleBottomPanel();
    expect(store.getState().isBottomPanelOpen).toBe(false);
  });

  it("analyzeRightTab defaults to results", () => {
    expect(store.getState().analyzeRightTab).toBe("results");
  });

  it("setAnalyzeRightTab changes tab", () => {
    store.getState().setAnalyzeRightTab("query");
    expect(store.getState().analyzeRightTab).toBe("query");
    store.getState().setAnalyzeRightTab("history");
    expect(store.getState().analyzeRightTab).toBe("history");
  });
});
