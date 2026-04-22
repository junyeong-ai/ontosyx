import type { AppStore } from "./types";

// ---------------------------------------------------------------------------
// Derived selectors (compute values from state)
// ---------------------------------------------------------------------------

export const selectStateHasOntology = (s: AppStore) => s.ontology !== null;
export const selectStateHasUnsavedEdits = (s: AppStore) => s.commandStack.length > 0;
export const selectStateSelectedNodeId = (s: AppStore) =>
  s.selection.type === "node" ? s.selection.nodeId : null;
export const selectStateSelectedEdgeId = (s: AppStore) =>
  s.selection.type === "edge" ? s.selection.edgeId : null;
export const selectStateSelectedWidgetId = (s: AppStore) =>
  s.selection.type === "widget" ? s.selection.widgetId : null;
export const selectStateCanChat = (s: AppStore) => s.ontology !== null;

// ---------------------------------------------------------------------------
// State selectors — OntologySlice
// ---------------------------------------------------------------------------

export const selectStateOntology = (s: AppStore) => s.ontology;
export const selectStateCommandStack = (s: AppStore) => s.commandStack;
export const selectStateRedoStack = (s: AppStore) => s.redoStack;
export const selectStateNodeGroups = (s: AppStore) => s.nodeGroups;

// ---------------------------------------------------------------------------
// State selectors — ChatSlice
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
// State selectors — ProjectSlice
// ---------------------------------------------------------------------------

export const selectStateActiveProject = (s: AppStore) => s.activeProject;
export const selectStateLastReconcileReport = (s: AppStore) =>
  s.lastReconcileReport;
export const selectStatePendingReconcile = (s: AppStore) => s.pendingReconcile;
export const selectStateActiveDiffOverlay = (s: AppStore) => s.activeDiffOverlay;

// ---------------------------------------------------------------------------
// State selectors — ChromeSlice
// ---------------------------------------------------------------------------

// `selectWorkspaceMode` removed (Phase 2-4). Use `useWorkspaceMode()`
// from `@/lib/use-workspace-mode` — the URL is the source of truth.
export const selectStateDesignBottomTab = (s: AppStore) => s.designBottomTab;
export const selectStateIsExplorerOpen = (s: AppStore) => s.isExplorerOpen;
export const selectStateIsInspectorOpen = (s: AppStore) => s.isInspectorOpen;
export const selectStateIsBottomPanelOpen = (s: AppStore) => s.isBottomPanelOpen;
export const selectStateAnalyzeRightTab = (s: AppStore) => s.analyzeRightTab;
export const selectStateOntologyId = (s: AppStore) => s.ontologyId;

// ---------------------------------------------------------------------------
// State selectors — SelectionSlice
// ---------------------------------------------------------------------------

export const selectStateSelection = (s: AppStore) => s.selection;
export const selectStateNeighborhoodFocus = (s: AppStore) => s.neighborhoodFocus;

// ---------------------------------------------------------------------------
// State selectors — DashboardSlice
// ---------------------------------------------------------------------------

export const selectStateActiveDashboardId = (s: AppStore) => s.activeDashboardId;
export const selectStateDashboardWidgetCount = (s: AppStore) =>
  s.dashboardWidgetCount;
export const selectStateDashboardFilters = (s: AppStore) => s.dashboardFilters;
export const selectStateDashboardTypeFilters = (s: AppStore) =>
  s.dashboardTypeFilters;

// ---------------------------------------------------------------------------
// Action selectors — OntologySlice
// ---------------------------------------------------------------------------

export const selectActionSetOntology = (s: AppStore) => s.setOntology;
export const selectActionApplyCommand = (s: AppStore) => s.applyCommand;
export const selectActionUndo = (s: AppStore) => s.undo;
export const selectActionRedo = (s: AppStore) => s.redo;
export const selectActionClearCommandStack = (s: AppStore) => s.clearCommandStack;
export const selectActionResetOntology = (s: AppStore) => s.resetOntology;
export const selectActionLoadOntology = (s: AppStore) => s.loadOntology;
export const selectActionRestoreNodeGroups = (s: AppStore) => s.restoreNodeGroups;
export const selectActionCreateGroup = (s: AppStore) => s.createGroup;
export const selectActionToggleGroupCollapse = (s: AppStore) =>
  s.toggleGroupCollapse;
export const selectActionRemoveGroup = (s: AppStore) => s.removeGroup;
export const selectActionRenameGroup = (s: AppStore) => s.renameGroup;

// ---------------------------------------------------------------------------
// Action selectors — ChatSlice
// ---------------------------------------------------------------------------

export const selectActionSetSessionId = (s: AppStore) => s.setSessionId;
export const selectActionAddMessage = (s: AppStore) => s.addMessage;
export const selectActionUpdateMessage = (s: AppStore) => s.updateMessage;
export const selectActionRestoreMessages = (s: AppStore) => s.restoreMessages;
export const selectActionClearMessages = (s: AppStore) => s.clearMessages;
export const selectActionSetIsLoading = (s: AppStore) => s.setIsLoading;
export const selectActionSetTokenUsage = (s: AppStore) => s.setTokenUsage;
export const selectActionSetHighlightedBindings = (s: AppStore) =>
  s.setHighlightedBindings;
export const selectActionSetCommandBarInput = (s: AppStore) => s.setCommandBarInput;
export const selectActionTakeCommandBarInput = (s: AppStore) =>
  s.takeCommandBarInput;
export const selectActionSetExecutionMode = (s: AppStore) => s.setExecutionMode;
export const selectActionSetModelOverride = (s: AppStore) => s.setModelOverride;

// ---------------------------------------------------------------------------
// Action selectors — ProjectSlice
// ---------------------------------------------------------------------------

export const selectActionSetActiveProject = (s: AppStore) => s.setActiveProject;
export const selectActionSetLastReconcileReport = (s: AppStore) =>
  s.setLastReconcileReport;
export const selectActionSetPendingReconcile = (s: AppStore) =>
  s.setPendingReconcile;
export const selectActionSetActiveDiffOverlay = (s: AppStore) =>
  s.setActiveDiffOverlay;

// ---------------------------------------------------------------------------
// Action selectors — ChromeSlice
// ---------------------------------------------------------------------------

// `selectSetWorkspaceMode` removed (Phase 2-4). Navigate with
// `router.push("/design")` etc. — Zustand no longer owns the mode.
export const selectActionSetDesignBottomTab = (s: AppStore) => s.setDesignBottomTab;
export const selectActionToggleExplorer = (s: AppStore) => s.toggleExplorer;
export const selectActionToggleInspector = (s: AppStore) => s.toggleInspector;
export const selectActionToggleBottomPanel = (s: AppStore) => s.toggleBottomPanel;
export const selectActionSetAnalyzeRightTab = (s: AppStore) => s.setAnalyzeRightTab;
export const selectActionSetOntologyId = (s: AppStore) => s.setOntologyId;
export const selectStateFocusResultId = (s: AppStore) => s.focusResultId;
export const selectActionSetFocusResultId = (s: AppStore) => s.setFocusResultId;

// ---------------------------------------------------------------------------
// Action selectors — SelectionSlice
// ---------------------------------------------------------------------------

export const selectActionSelect = (s: AppStore) => s.select;
export const selectActionClearSelection = (s: AppStore) => s.clearSelection;
export const selectActionSetNeighborhoodFocus = (s: AppStore) =>
  s.setNeighborhoodFocus;

// ---------------------------------------------------------------------------
// Action selectors — DashboardSlice
// ---------------------------------------------------------------------------

export const selectActionSetActiveDashboardId = (s: AppStore) =>
  s.setActiveDashboardId;
export const selectActionSetDashboardWidgetCount = (s: AppStore) =>
  s.setDashboardWidgetCount;
export const selectActionSetDashboardFilter = (s: AppStore) => s.setDashboardFilter;
export const selectActionClearDashboardFilters = (s: AppStore) =>
  s.clearDashboardFilters;
export const selectActionToggleDashboardType = (s: AppStore) => s.toggleDashboardType;
export const selectActionSetDashboardTypeHidden = (s: AppStore) =>
  s.setDashboardTypeHidden;
export const selectActionClearDashboardTypes = (s: AppStore) => s.clearDashboardTypes;

// ---------------------------------------------------------------------------
// State selectors — VerificationSlice
// ---------------------------------------------------------------------------

export const selectStateVerifications = (s: AppStore) => s.verifications;
export const selectStateVerificationsLoading = (s: AppStore) =>
  s.verificationsLoading;

// ---------------------------------------------------------------------------
// Action selectors — VerificationSlice
// ---------------------------------------------------------------------------

export const selectActionLoadVerifications = (s: AppStore) => s.loadVerifications;
export const selectActionVerifyElement = (s: AppStore) => s.verifyElement;
export const selectActionRevokeVerification = (s: AppStore) => s.revokeVerification;
export const selectActionClearVerifications = (s: AppStore) => s.clearVerifications;
