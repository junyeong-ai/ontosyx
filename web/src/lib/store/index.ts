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
  WorkspaceMode,
  DesignBottomTab,
  AnalyzeRightTab,
  Selection,
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
  // Derived selectors
  selectHasOntology,
  selectHasUnsavedEdits,
  selectSelectedNodeId,
  selectSelectedEdgeId,
  selectSelectedWidgetId,
  selectCanChat,
  // State — OntologySlice
  selectOntology,
  selectCommandStack,
  selectRedoStack,
  selectNodeGroups,
  // State — ChatSlice
  selectMessages,
  selectIsLoading,
  selectSessionId,
  selectTokenUsage,
  selectHighlightedBindings,
  selectPendingCommandBarInput,
  selectExecutionMode,
  // State — ProjectSlice
  selectActiveProject,
  selectLastReconcileReport,
  selectPendingReconcile,
  selectActiveDiffOverlay,
  // State — ChromeSlice
  selectWorkspaceMode,
  selectDesignBottomTab,
  selectIsExplorerOpen,
  selectIsInspectorOpen,
  selectIsBottomPanelOpen,
  selectAnalyzeRightTab,
  selectSavedOntologyId,
  // State — SelectionSlice
  selectSelection,
  selectNeighborhoodFocus,
  // State — DashboardSlice
  selectActiveDashboardId,
  selectDashboardWidgetCount,
  selectDashboardFilters,
  // Actions — OntologySlice
  selectSetOntology,
  selectApplyCommand,
  selectUndo,
  selectRedo,
  selectClearCommandStack,
  selectResetOntology,
  selectLoadSavedOntology,
  selectRestoreNodeGroups,
  selectCreateGroup,
  selectToggleGroupCollapse,
  selectRemoveGroup,
  selectRenameGroup,
  // Actions — ChatSlice
  selectSetSessionId,
  selectAddMessage,
  selectUpdateMessage,
  selectRestoreMessages,
  selectClearMessages,
  selectSetIsLoading,
  selectSetTokenUsage,
  selectSetHighlightedBindings,
  selectSetCommandBarInput,
  selectTakeCommandBarInput,
  selectSetExecutionMode,
  selectModelOverride,
  selectSetModelOverride,
  // Actions — ProjectSlice
  selectSetActiveProject,
  selectSetLastReconcileReport,
  selectSetPendingReconcile,
  selectSetActiveDiffOverlay,
  // Actions — ChromeSlice
  selectSetWorkspaceMode,
  selectSetDesignBottomTab,
  selectToggleExplorer,
  selectToggleInspector,
  selectToggleBottomPanel,
  selectSetAnalyzeRightTab,
  selectSetSavedOntologyId,
  selectFocusResultId,
  selectSetFocusResultId,
  // Actions — SelectionSlice
  selectSelect,
  selectClearSelection,
  selectSetNeighborhoodFocus,
  // Actions — DashboardSlice
  selectSetActiveDashboardId,
  selectSetDashboardWidgetCount,
  selectSetDashboardFilter,
  selectClearDashboardFilters,
  // State — VerificationSlice
  selectVerifications,
  selectVerificationsLoading,
  // Actions — VerificationSlice
  selectLoadVerifications,
  selectVerifyElement,
  selectRevokeVerification,
  selectClearVerifications,
} from "./selectors";

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
        // Only persist UI layout preferences — NOT workspace-scoped data.
        // savedOntologyId was removed: it's workspace-scoped and gets stale
        // when switching workspaces. Analyze/Explore modes re-fetch on mount.
        workspaceMode: state.workspaceMode,
        designBottomTab: state.designBottomTab,
        analyzeRightTab: state.analyzeRightTab,
        isExplorerOpen: state.isExplorerOpen,
        isInspectorOpen: state.isInspectorOpen,
        isBottomPanelOpen: state.isBottomPanelOpen,
      }),
    },
  ),
);

// Expose store for DevTools debugging (development only)
if (typeof window !== "undefined" && process.env.NODE_ENV === "development") {
  (window as unknown as Record<string, unknown>).__appStore = useAppStore;
}
