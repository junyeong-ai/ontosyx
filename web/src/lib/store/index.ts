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
  selectStateHasOntology,
  selectStateHasUnsavedEdits,
  selectStateSelectedNodeId,
  selectStateSelectedEdgeId,
  selectStateSelectedWidgetId,
  selectStateCanChat,
  // State — OntologySlice
  selectStateOntology,
  selectStateCommandStack,
  selectStateRedoStack,
  selectStateNodeGroups,
  // State — ChatSlice
  selectStateMessages,
  selectStateIsLoading,
  selectStateSessionId,
  selectStateTokenUsage,
  selectStateHighlightedBindings,
  selectStatePendingCommandBarInput,
  selectStateExecutionMode,
  // State — ProjectSlice
  selectStateActiveProject,
  selectStateLastReconcileReport,
  selectStatePendingReconcile,
  selectStateActiveDiffOverlay,
  // State — ChromeSlice
  selectStateDesignBottomTab,
  selectStateIsExplorerOpen,
  selectStateIsInspectorOpen,
  selectStateIsBottomPanelOpen,
  selectStateAnalyzeRightTab,
  selectStateOntologyId,
  // State — SelectionSlice
  selectStateSelection,
  selectStateNeighborhoodFocus,
  // State — DashboardSlice
  selectStateActiveDashboardId,
  selectStateDashboardWidgetCount,
  selectStateDashboardFilters,
  selectStateDashboardTypeFilters,
  // Actions — OntologySlice
  selectActionSetOntology,
  selectActionApplyCommand,
  selectActionUndo,
  selectActionRedo,
  selectActionClearCommandStack,
  selectActionResetOntology,
  selectActionLoadOntology,
  selectActionRestoreNodeGroups,
  selectActionCreateGroup,
  selectActionToggleGroupCollapse,
  selectActionRemoveGroup,
  selectActionRenameGroup,
  // Actions — ChatSlice
  selectActionSetSessionId,
  selectActionAddMessage,
  selectActionUpdateMessage,
  selectActionRestoreMessages,
  selectActionClearMessages,
  selectActionSetIsLoading,
  selectActionSetTokenUsage,
  selectActionSetHighlightedBindings,
  selectActionSetCommandBarInput,
  selectActionTakeCommandBarInput,
  selectActionSetExecutionMode,
  selectStateModelOverride,
  selectActionSetModelOverride,
  // Actions — ProjectSlice
  selectActionSetActiveProject,
  selectActionSetLastReconcileReport,
  selectActionSetPendingReconcile,
  selectActionSetActiveDiffOverlay,
  // Actions — ChromeSlice
  selectActionSetDesignBottomTab,
  selectActionToggleExplorer,
  selectActionToggleInspector,
  selectActionToggleBottomPanel,
  selectActionSetAnalyzeRightTab,
  selectActionSetOntologyId,
  selectStateFocusResultId,
  selectActionSetFocusResultId,
  // Actions — SelectionSlice
  selectActionSelect,
  selectActionClearSelection,
  selectActionSetNeighborhoodFocus,
  // Actions — DashboardSlice
  selectActionSetActiveDashboardId,
  selectActionSetDashboardWidgetCount,
  selectActionSetDashboardFilter,
  selectActionClearDashboardFilters,
  selectActionToggleDashboardType,
  selectActionSetDashboardTypeHidden,
  selectActionClearDashboardTypes,
  // State — VerificationSlice
  selectStateVerifications,
  selectStateVerificationsLoading,
  // Actions — VerificationSlice
  selectActionLoadVerifications,
  selectActionVerifyElement,
  selectActionRevokeVerification,
  selectActionClearVerifications,
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
        // ontologyId was removed: it's workspace-scoped and gets stale
        // when switching workspaces. Analyze/Explore modes re-fetch on mount.
        // `workspaceMode` removed in Phase 2-4 — the active mode now
        // derives from the URL, so persisting it would desync from
        // navigation on reload.
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
