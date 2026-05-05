import type { StateCreator } from "zustand";
import type { AppStore, BottomPanelMode, ChromeSlice } from "./types";
import {
  getWorkspaceId,
  getWorkspaceName,
  setWorkspaceId,
  setWorkspaceName,
  setWorkspaceRole,
} from "@/lib/workspace";

export const createChromeSlice: StateCreator<AppStore, [], [], ChromeSlice> = (set) => ({
  // Active workspace — synced with localStorage
  workspaceId: null,
  workspaceName: null,
  workspaceReady: false,

  initWorkspace: async () => {
    // 1. Check localStorage first (synchronous, instant)
    const storedId = getWorkspaceId();
    const storedName = getWorkspaceName();
    if (storedId) {
      set({ workspaceId: storedId, workspaceName: storedName ?? null, workspaceReady: true });
      return;
    }

    // 2. No stored preference → fetch workspaces and auto-select default
    try {
      const { listWorkspaces: fetchWorkspaces } = await import("@/lib/api/workspaces");
      const workspaces = await fetchWorkspaces();
      if (workspaces.length > 0) {
        const ws = workspaces[0]; // Server returns default first
        setWorkspaceId(ws.id);
        setWorkspaceName(ws.name);
        setWorkspaceRole(ws.role);
        set({ workspaceId: ws.id, workspaceName: ws.name, workspaceReady: true });
      } else {
        // No workspaces available
        set({ workspaceReady: true });
      }
    } catch {
      // API failed — proceed without workspace (backend will use default fallback)
      set({ workspaceReady: true });
    }
  },

  setActiveWorkspace: (id, name, role) => {
    setWorkspaceId(id);
    setWorkspaceName(name);
    setWorkspaceRole(role);
    set({ workspaceId: id, workspaceName: name });
  },

  designBottomTab: "workflow",
  setDesignBottomTab: (tab) => set({ designBottomTab: tab }),

  isExplorerOpen: true,
  isInspectorOpen: true,
  isBottomPanelOpen: true,
  isSearchOpen: false,
  isCommandPaletteOpen: false,
  toggleExplorer: () => set((s) => ({ isExplorerOpen: !s.isExplorerOpen })),
  toggleInspector: () => set((s) => ({ isInspectorOpen: !s.isInspectorOpen })),
  toggleBottomPanel: () => set((s) => ({ isBottomPanelOpen: !s.isBottomPanelOpen })),
  setSearchOpen: (open) => set({ isSearchOpen: open }),
  setCommandPaletteOpen: (open) => set({ isCommandPaletteOpen: open }),

  extendSourceRequestCount: 0,
  requestExtendSource: () =>
    set((s) => ({ extendSourceRequestCount: s.extendSourceRequestCount + 1 })),

  bottomPanelMode: "default",
  setBottomPanelMode: (mode) => set({ bottomPanelMode: mode, isBottomPanelOpen: true }),
  cycleBottomPanelMode: () =>
    set((s) => {
      const next: BottomPanelMode =
        s.bottomPanelMode === "default"
          ? "tall"
          : s.bottomPanelMode === "tall"
            ? "fullscreen"
            : "default";
      return { bottomPanelMode: next, isBottomPanelOpen: true };
    }),

  sidebarMode: "rail",
  toggleSidebarMode: () =>
    set((s) => ({ sidebarMode: s.sidebarMode === "rail" ? "expanded" : "rail" })),

  inspectorTabByKind: {},
  setInspectorTab: (kind, tab) =>
    set((s) => ({
      inspectorTabByKind: { ...s.inspectorTabByKind, [kind]: tab },
    })),

  analyzeRightTab: "results",
  setAnalyzeRightTab: (tab) => set({ analyzeRightTab: tab }),

  ontologyId: null,
  setOntologyId: (id) => set({ ontologyId: id }),

  focusResultId: null,
  setFocusResultId: (id) => set({ focusResultId: id }),
});
