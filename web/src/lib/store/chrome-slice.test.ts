import { describe, it, expect, beforeEach } from "vitest";
import { createStore } from "zustand";
import type { AppStore } from "./types";
import { createOntologySlice } from "./ontology-slice";
import { createChatSlice } from "./chat-slice";
import { createChromeSlice } from "./chrome-slice";
import { createSelectionSlice } from "./selection-slice";
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

describe("ChromeSlice", () => {
  let store: ReturnType<typeof createTestStore>;

  beforeEach(() => {
    store = createTestStore();
  });

  // `workspaceMode` is URL-derived (`useWorkspaceMode()`); tests for
  // pathname-derived mode live with that hook, not here.

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
