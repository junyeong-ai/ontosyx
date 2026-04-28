import type { AppStore } from "./types";

// State selectors return raw or derived state slices for use with
// `useAppStore(selector)`. Action handles are read inline at the
// call site (`useAppStore((s) => s.fooBar)`) — wrapping them adds
// no memoization value and obscures the canonical Zustand pattern.

// ---------------------------------------------------------------------------
// Derived selectors
// ---------------------------------------------------------------------------

export const selectStateHasOntology = (s: AppStore) => s.ontology !== null;
export const selectStateHasUnsavedEdits = (s: AppStore) =>
  s.commandStack.length > 0;
export const selectStateSelectedNodeId = (s: AppStore) =>
  s.selection.type === "node" ? s.selection.nodeId : null;
export const selectStateSelectedEdgeId = (s: AppStore) =>
  s.selection.type === "edge" ? s.selection.edgeId : null;
export const selectStateSelectedWidgetId = (s: AppStore) =>
  s.selection.type === "widget" ? s.selection.widgetId : null;
export const selectStateCanChat = (s: AppStore) => s.ontology !== null;

// ---------------------------------------------------------------------------
// OntologySlice
// ---------------------------------------------------------------------------

export const selectStateOntology = (s: AppStore) => s.ontology;
export const selectStateCommandStack = (s: AppStore) => s.commandStack;
export const selectStateRedoStack = (s: AppStore) => s.redoStack;
export const selectStateNodeGroups = (s: AppStore) => s.nodeGroups;

// ---------------------------------------------------------------------------
// ChatSlice
// ---------------------------------------------------------------------------

export const selectStateMessages = (s: AppStore) => s.messages;
export const selectStateIsLoading = (s: AppStore) => s.isLoading;
export const selectStateSessionId = (s: AppStore) => s.sessionId;
export const selectStateTokenUsage = (s: AppStore) => s.tokenUsage;
export const selectStateHighlightedBindings = (s: AppStore) =>
  s.highlightedBindings;
export const selectStatePendingCommandBarInput = (s: AppStore) =>
  s.pendingCommandBarInput;
export const selectStateExecutionMode = (s: AppStore) => s.executionMode;
export const selectStateModelOverride = (s: AppStore) => s.modelOverride;

// ---------------------------------------------------------------------------
// ProjectSlice
// ---------------------------------------------------------------------------

export const selectStateActiveProject = (s: AppStore) => s.activeProject;
export const selectStateLastReconcileReport = (s: AppStore) =>
  s.lastReconcileReport;
export const selectStatePendingReconcile = (s: AppStore) => s.pendingReconcile;
export const selectStateActiveDiffOverlay = (s: AppStore) =>
  s.activeDiffOverlay;

// ---------------------------------------------------------------------------
// ChromeSlice
// ---------------------------------------------------------------------------

export const selectStateDesignBottomTab = (s: AppStore) => s.designBottomTab;
export const selectStateIsExplorerOpen = (s: AppStore) => s.isExplorerOpen;
export const selectStateIsInspectorOpen = (s: AppStore) => s.isInspectorOpen;
export const selectStateIsBottomPanelOpen = (s: AppStore) =>
  s.isBottomPanelOpen;
export const selectStateAnalyzeRightTab = (s: AppStore) => s.analyzeRightTab;
export const selectStateOntologyId = (s: AppStore) => s.ontologyId;
export const selectStateFocusResultId = (s: AppStore) => s.focusResultId;

// ---------------------------------------------------------------------------
// SelectionSlice
// ---------------------------------------------------------------------------

export const selectStateSelection = (s: AppStore) => s.selection;
export const selectStateNeighborhoodFocus = (s: AppStore) =>
  s.neighborhoodFocus;

// ---------------------------------------------------------------------------
// DashboardSlice
// ---------------------------------------------------------------------------

export const selectStateActiveDashboardId = (s: AppStore) =>
  s.activeDashboardId;
export const selectStateDashboardFilters = (s: AppStore) => s.dashboardFilters;
export const selectStateDashboardTypeFilters = (s: AppStore) =>
  s.dashboardTypeFilters;

// ---------------------------------------------------------------------------
// VerificationSlice
// ---------------------------------------------------------------------------

export const selectStateVerifications = (s: AppStore) => s.verifications;
export const selectStateVerificationsLoading = (s: AppStore) =>
  s.verificationsLoading;
