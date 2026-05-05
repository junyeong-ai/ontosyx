"use client";

import { create } from "zustand";
import { persist } from "zustand/middleware";
import type { AppStore } from "./types";
import { createOntologySlice } from "./ontology-slice";
import { createChatSlice } from "./chat-slice";
import { createProjectSlice } from "./project-slice";
import { createChromeSlice } from "./chrome-slice";
import { createSelectionSlice } from "./selection-slice";
import { createDashboardSlice } from "./dashboard-slice";
import { createVerificationSlice } from "./verification-slice";

export type {
  AppStore,
  NodeGroup,
  NeighborhoodFocus,
  ChatMessage,
  ToolCall,
  ToolStep,
  WorkspaceMode,
  DesignBottomTab,
  BottomPanelMode,
  AnalyzeRightTab,
  Selection,
  SelectionKind,
  SelectionRef,
  CommandEntry,
  OntologySlice,
  ChatSlice,
  ProjectSlice,
  ChromeSlice,
  SelectionSlice,
  DashboardSlice,
  VerificationSlice,
} from "./types";

export {
  refKey,
  selectionContains,
  selectionContainsId,
  selectionOfKind,
  selectionPrimary,
} from "./selection-slice";

export * from "./selectors";

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

export const useAppStore = create<AppStore>()(
  persist(
    (...a) => ({
      ...createOntologySlice(...a),
      ...createChatSlice(...a),
      ...createProjectSlice(...a),
      ...createChromeSlice(...a),
      ...createSelectionSlice(...a),
      ...createDashboardSlice(...a),
      ...createVerificationSlice(...a),
    }),
    {
      name: "ontosyx-ui",
      partialize: (state) => ({
        // Only persist UI layout preferences — workspace-scoped data
        // (ontologyId, mode) belongs to the URL or to per-workspace
        // refetches.
        designBottomTab: state.designBottomTab,
        analyzeRightTab: state.analyzeRightTab,
        isExplorerOpen: state.isExplorerOpen,
        isInspectorOpen: state.isInspectorOpen,
        isBottomPanelOpen: state.isBottomPanelOpen,
        bottomPanelMode: state.bottomPanelMode,
        sidebarMode: state.sidebarMode,
        inspectorTabByKind: state.inspectorTabByKind,
      }),
    },
  ),
);

// Expose store for DevTools debugging (development only)
if (typeof window !== "undefined" && process.env.NODE_ENV === "development") {
  (window as unknown as Record<string, unknown>).__appStore = useAppStore;
}
