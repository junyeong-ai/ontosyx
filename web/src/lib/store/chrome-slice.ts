import type { StateCreator } from "zustand";
import type { AppStore, ChromeSlice } from "./types";

export const createChromeSlice: StateCreator<AppStore, [], [], ChromeSlice> = (set, get) => ({
  workspaceMode: "design",
  setWorkspaceMode: (mode) => {
    set({ workspaceMode: mode });
    // Reset selection when switching modes (via selection slice)
    get().clearSelection();
  },

  designBottomTab: "workflow",
  setDesignBottomTab: (tab) => set({ designBottomTab: tab }),

  isExplorerOpen: true,
  isInspectorOpen: true,
  isBottomPanelOpen: true,
  toggleExplorer: () => set((s) => ({ isExplorerOpen: !s.isExplorerOpen })),
  toggleInspector: () => set((s) => ({ isInspectorOpen: !s.isInspectorOpen })),
  toggleBottomPanel: () => set((s) => ({ isBottomPanelOpen: !s.isBottomPanelOpen })),

  analyzeRightTab: "results",
  setAnalyzeRightTab: (tab) => set({ analyzeRightTab: tab }),

  savedOntologyId: null,
  setSavedOntologyId: (id) => set({ savedOntologyId: id }),

  focusResultId: null,
  setFocusResultId: (id) => set({ focusResultId: id }),
});
