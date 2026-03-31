import type { AppStore } from "./types";

// ---------------------------------------------------------------------------
// Derived selectors (compute values from state)
// ---------------------------------------------------------------------------

export const selectHasOntology = (s: AppStore) => s.ontology !== null;
export const selectHasUnsavedEdits = (s: AppStore) => s.commandStack.length > 0;
export const selectSelectedNodeId = (s: AppStore) =>
  s.selection.type === "node" ? s.selection.nodeId : null;
export const selectSelectedEdgeId = (s: AppStore) =>
  s.selection.type === "edge" ? s.selection.edgeId : null;
export const selectSelectedWidgetId = (s: AppStore) =>
  s.selection.type === "widget" ? s.selection.widgetId : null;
export const selectCanChat = (s: AppStore) => s.ontology !== null;

// ---------------------------------------------------------------------------
// State selectors — OntologySlice
// ---------------------------------------------------------------------------

export const selectOntology = (s: AppStore) => s.ontology;
export const selectCommandStack = (s: AppStore) => s.commandStack;
export const selectRedoStack = (s: AppStore) => s.redoStack;
export const selectNodeGroups = (s: AppStore) => s.nodeGroups;

// ---------------------------------------------------------------------------
// State selectors — ChatSlice
// ---------------------------------------------------------------------------

export const selectMessages = (s: AppStore) => s.messages;
export const selectIsLoading = (s: AppStore) => s.isLoading;
export const selectSessionId = (s: AppStore) => s.sessionId;
export const selectTokenUsage = (s: AppStore) => s.tokenUsage;
export const selectHighlightedBindings = (s: AppStore) =>
  s.highlightedBindings;
export const selectPendingCommandBarInput = (s: AppStore) =>
  s.pendingCommandBarInput;
export const selectExecutionMode = (s: AppStore) => s.executionMode;
export const selectModelOverride = (s: AppStore) => s.modelOverride;

// ---------------------------------------------------------------------------
// State selectors — ProjectSlice
// ---------------------------------------------------------------------------

export const selectActiveProject = (s: AppStore) => s.activeProject;
export const selectLastReconcileReport = (s: AppStore) =>
  s.lastReconcileReport;
export const selectPendingReconcile = (s: AppStore) => s.pendingReconcile;
export const selectActiveDiffOverlay = (s: AppStore) => s.activeDiffOverlay;

// ---------------------------------------------------------------------------
// State selectors — ChromeSlice
// ---------------------------------------------------------------------------

export const selectWorkspaceMode = (s: AppStore) => s.workspaceMode;
export const selectDesignBottomTab = (s: AppStore) => s.designBottomTab;
export const selectIsExplorerOpen = (s: AppStore) => s.isExplorerOpen;
export const selectIsInspectorOpen = (s: AppStore) => s.isInspectorOpen;
export const selectIsBottomPanelOpen = (s: AppStore) => s.isBottomPanelOpen;
export const selectAnalyzeRightTab = (s: AppStore) => s.analyzeRightTab;
export const selectSavedOntologyId = (s: AppStore) => s.savedOntologyId;

// ---------------------------------------------------------------------------
// State selectors — SelectionSlice
// ---------------------------------------------------------------------------

export const selectSelection = (s: AppStore) => s.selection;
export const selectNeighborhoodFocus = (s: AppStore) => s.neighborhoodFocus;

// ---------------------------------------------------------------------------
// State selectors — DashboardSlice
// ---------------------------------------------------------------------------

export const selectActiveDashboardId = (s: AppStore) => s.activeDashboardId;
export const selectDashboardWidgetCount = (s: AppStore) =>
  s.dashboardWidgetCount;
export const selectDashboardFilters = (s: AppStore) => s.dashboardFilters;

// ---------------------------------------------------------------------------
// Action selectors — OntologySlice
// ---------------------------------------------------------------------------

export const selectSetOntology = (s: AppStore) => s.setOntology;
export const selectApplyCommand = (s: AppStore) => s.applyCommand;
export const selectUndo = (s: AppStore) => s.undo;
export const selectRedo = (s: AppStore) => s.redo;
export const selectClearCommandStack = (s: AppStore) => s.clearCommandStack;
export const selectResetOntology = (s: AppStore) => s.resetOntology;
export const selectLoadSavedOntology = (s: AppStore) => s.loadSavedOntology;
export const selectRestoreNodeGroups = (s: AppStore) => s.restoreNodeGroups;
export const selectCreateGroup = (s: AppStore) => s.createGroup;
export const selectToggleGroupCollapse = (s: AppStore) =>
  s.toggleGroupCollapse;
export const selectRemoveGroup = (s: AppStore) => s.removeGroup;
export const selectRenameGroup = (s: AppStore) => s.renameGroup;

// ---------------------------------------------------------------------------
// Action selectors — ChatSlice
// ---------------------------------------------------------------------------

export const selectSetSessionId = (s: AppStore) => s.setSessionId;
export const selectAddMessage = (s: AppStore) => s.addMessage;
export const selectUpdateMessage = (s: AppStore) => s.updateMessage;
export const selectRestoreMessages = (s: AppStore) => s.restoreMessages;
export const selectClearMessages = (s: AppStore) => s.clearMessages;
export const selectSetIsLoading = (s: AppStore) => s.setIsLoading;
export const selectSetTokenUsage = (s: AppStore) => s.setTokenUsage;
export const selectSetHighlightedBindings = (s: AppStore) =>
  s.setHighlightedBindings;
export const selectSetCommandBarInput = (s: AppStore) => s.setCommandBarInput;
export const selectTakeCommandBarInput = (s: AppStore) =>
  s.takeCommandBarInput;
export const selectSetExecutionMode = (s: AppStore) => s.setExecutionMode;
export const selectSetModelOverride = (s: AppStore) => s.setModelOverride;

// ---------------------------------------------------------------------------
// Action selectors — ProjectSlice
// ---------------------------------------------------------------------------

export const selectSetActiveProject = (s: AppStore) => s.setActiveProject;
export const selectSetLastReconcileReport = (s: AppStore) =>
  s.setLastReconcileReport;
export const selectSetPendingReconcile = (s: AppStore) =>
  s.setPendingReconcile;
export const selectSetActiveDiffOverlay = (s: AppStore) =>
  s.setActiveDiffOverlay;

// ---------------------------------------------------------------------------
// Action selectors — ChromeSlice
// ---------------------------------------------------------------------------

export const selectSetWorkspaceMode = (s: AppStore) => s.setWorkspaceMode;
export const selectSetDesignBottomTab = (s: AppStore) => s.setDesignBottomTab;
export const selectToggleExplorer = (s: AppStore) => s.toggleExplorer;
export const selectToggleInspector = (s: AppStore) => s.toggleInspector;
export const selectToggleBottomPanel = (s: AppStore) => s.toggleBottomPanel;
export const selectSetAnalyzeRightTab = (s: AppStore) => s.setAnalyzeRightTab;
export const selectSetSavedOntologyId = (s: AppStore) => s.setSavedOntologyId;
export const selectFocusResultId = (s: AppStore) => s.focusResultId;
export const selectSetFocusResultId = (s: AppStore) => s.setFocusResultId;

// ---------------------------------------------------------------------------
// Action selectors — SelectionSlice
// ---------------------------------------------------------------------------

export const selectSelect = (s: AppStore) => s.select;
export const selectClearSelection = (s: AppStore) => s.clearSelection;
export const selectSetNeighborhoodFocus = (s: AppStore) =>
  s.setNeighborhoodFocus;

// ---------------------------------------------------------------------------
// Action selectors — DashboardSlice
// ---------------------------------------------------------------------------

export const selectSetActiveDashboardId = (s: AppStore) =>
  s.setActiveDashboardId;
export const selectSetDashboardWidgetCount = (s: AppStore) =>
  s.setDashboardWidgetCount;
export const selectSetDashboardFilter = (s: AppStore) => s.setDashboardFilter;
export const selectClearDashboardFilters = (s: AppStore) =>
  s.clearDashboardFilters;

// ---------------------------------------------------------------------------
// State selectors — VerificationSlice
// ---------------------------------------------------------------------------

export const selectVerifications = (s: AppStore) => s.verifications;
export const selectVerificationsLoading = (s: AppStore) =>
  s.verificationsLoading;

// ---------------------------------------------------------------------------
// Action selectors — VerificationSlice
// ---------------------------------------------------------------------------

export const selectLoadVerifications = (s: AppStore) => s.loadVerifications;
export const selectVerifyElement = (s: AppStore) => s.verifyElement;
export const selectRevokeVerification = (s: AppStore) => s.revokeVerification;
export const selectClearVerifications = (s: AppStore) => s.clearVerifications;
