import type {
  OntologyIR,
  OntologyCommand,
  OntologyDiff,
  DesignProject,
  ElementVerification,
  ResolvedQueryBindings,
  ReconcileReport,
  PendingReconcile,
} from "@/types/api";

// ---------------------------------------------------------------------------
// Node group definition (for grouping/folding)
// ---------------------------------------------------------------------------

export interface NodeGroup {
  name: string;
  nodeIds: string[];
  collapsed: boolean;
  color?: string;
}

// ---------------------------------------------------------------------------
// Neighborhood focus (for path exploration)
// ---------------------------------------------------------------------------

export interface NeighborhoodFocus {
  nodeId: string;
  depth: number;
}

// ---------------------------------------------------------------------------
// Chat message types
// ---------------------------------------------------------------------------

export interface ToolStep {
  step: string;
  status: "started" | "completed" | "failed";
  durationMs?: number;
  metadata?: Record<string, unknown>;
}

export interface ToolCall {
  id: string;
  name: string;
  input?: unknown;
  output?: string;
  status: "running" | "done" | "error" | "review";
  durationMs?: number;
  /** Sub-step progress for long-running tools (e.g., query_graph). */
  steps?: ToolStep[];
}

export interface ChatMessage {
  id: string;
  role: "user" | "assistant";
  content: string;
  /** Chain-of-thought reasoning (from LLM thinking events). */
  thinking?: string;
  toolCalls?: ToolCall[];
  isStreaming?: boolean;
  error?: string;
}

// ---------------------------------------------------------------------------
// Workspace mode — top-level layout orchestrator
// ---------------------------------------------------------------------------

export type WorkspaceMode = "design" | "analyze" | "explore" | "dashboard";

// ---------------------------------------------------------------------------
// Design mode sub-tabs (bottom panel within Design workspace)
// ---------------------------------------------------------------------------

export type DesignBottomTab = "chat" | "workflow" | "quality";

/**
 * Snap presets for the bottom panel. `default` is ~30%; `tall` is
 * ~70%; `fullscreen` covers the viewport. The exact percentages
 * live in `chrome-slice` so tweaking them is one edit.
 */
export type BottomPanelMode = "default" | "tall" | "fullscreen";

// ---------------------------------------------------------------------------
// Analyze mode right panel tabs
// ---------------------------------------------------------------------------

export type AnalyzeRightTab = "results" | "query" | "history" | "insights" | "knowledge";

// ---------------------------------------------------------------------------
// Unified selection model
// ---------------------------------------------------------------------------

export type Selection =
  | { type: "none" }
  | { type: "node"; nodeId: string }
  | { type: "edge"; edgeId: string }
  | { type: "widget"; widgetId: string };

// ---------------------------------------------------------------------------
// Command stack entry (for undo/redo)
// ---------------------------------------------------------------------------

export interface CommandEntry {
  command: OntologyCommand;
  inverse: OntologyCommand;
}

// ---------------------------------------------------------------------------
// Slice state interfaces
// ---------------------------------------------------------------------------

export interface OntologySlice {
  ontology: OntologyIR | null;
  setOntology: (ontology: OntologyIR) => void;
  commandStack: CommandEntry[];
  redoStack: CommandEntry[];
  applyCommand: (command: OntologyCommand) => void;
  undo: () => void;
  redo: () => void;
  clearCommandStack: () => void;
  resetOntology: () => void;
  loadOntology: (ontology: OntologyIR) => void;
  nodeGroups: Record<string, NodeGroup>;
  restoreNodeGroups: (groups: Record<string, NodeGroup>) => void;
  createGroup: (name: string, nodeIds: string[]) => void;
  toggleGroupCollapse: (groupId: string) => void;
  removeGroup: (groupId: string) => void;
  renameGroup: (groupId: string, name: string) => void;
}

export interface ChatSlice {
  messages: ChatMessage[];
  isLoading: boolean;
  sessionId: string | null;
  setSessionId: (id: string | null) => void;
  addMessage: (msg: ChatMessage) => void;
  updateMessage: (id: string, patch: Partial<ChatMessage>) => void;
  restoreMessages: (messages: ChatMessage[]) => void;
  clearMessages: () => void;
  setIsLoading: (isLoading: boolean) => void;
  tokenUsage: { input: number; output: number } | null;
  setTokenUsage: (usage: { input: number; output: number } | null) => void;
  highlightedBindings: ResolvedQueryBindings | null;
  setHighlightedBindings: (bindings: ResolvedQueryBindings | null) => void;
  pendingCommandBarInput: string | null;
  setCommandBarInput: (input: string) => void;
  takeCommandBarInput: () => string | null;
  executionMode: "auto" | "supervised";
  setExecutionMode: (mode: "auto" | "supervised") => void;
  modelOverride: string | null;
  setModelOverride: (model: string | null) => void;
}

export interface ProjectSlice {
  activeProject: DesignProject | null;
  setActiveProject: (project: DesignProject | null) => void;
  lastReconcileReport: ReconcileReport | null;
  setLastReconcileReport: (report: ReconcileReport | null) => void;
  pendingReconcile: PendingReconcile | null;
  setPendingReconcile: (reconcile: PendingReconcile | null) => void;
  activeDiffOverlay: OntologyDiff | null;
  setActiveDiffOverlay: (diff: OntologyDiff | null) => void;
}

export interface ChromeSlice {
  // Active workspace
  workspaceId: string | null;
  workspaceName: string | null;
  workspaceReady: boolean;
  initWorkspace: () => Promise<void>;
  setActiveWorkspace: (id: string, name: string, role: string) => void;

  // `workspaceMode` moved to URL (Phase 2-4). Consume via
  // `useWorkspaceMode()` — the slice no longer carries the state.

  // Design mode
  designBottomTab: DesignBottomTab;
  setDesignBottomTab: (tab: DesignBottomTab) => void;
  isExplorerOpen: boolean;
  isInspectorOpen: boolean;
  isBottomPanelOpen: boolean;
  /**
   * Global search-dialog visibility. Lives on the chrome slice so
   * any header / sidebar / palette button can drive it without the
   * `DesignLayout` owning local state and forcing prop drilling.
   * Cmd/Ctrl+K toggles, Esc closes — keybindings remain in
   * `DesignLayout` since that's where the dialog mounts.
   */
  isSearchOpen: boolean;
  /**
   * Global command-palette visibility. Cmd/Ctrl+Shift+P toggles
   * (mirrors VS Code's "Command Palette" binding); the palette
   * lives in the workbench layout root so every mode shares the
   * same instance.
   */
  isCommandPaletteOpen: boolean;
  toggleExplorer: () => void;
  toggleInspector: () => void;
  toggleBottomPanel: () => void;
  setSearchOpen: (open: boolean) => void;
  setCommandPaletteOpen: (open: boolean) => void;
  /**
   * Snap-point preference for the bottom panel. `default` is the
   * comfortable in-context size; `tall` doubles the panel for
   * heavy review sessions; `fullscreen` hides the canvas/review
   * pane entirely so the panel covers the viewport. The store
   * persists the preference; `DesignLayout` translates it into
   * a `defaultSize` on the resizable panel.
   */
  bottomPanelMode: BottomPanelMode;
  setBottomPanelMode: (mode: BottomPanelMode) => void;
  cycleBottomPanelMode: () => void;

  // Analyze mode
  analyzeRightTab: AnalyzeRightTab;
  setAnalyzeRightTab: (tab: AnalyzeRightTab) => void;
  /** Identity uuid (`ontologies.id`) of the ontology currently active in
   *  Analyze mode. `null` means no pinned ontology. */
  ontologyId: string | null;
  setOntologyId: (id: string | null) => void;

  /** Tool call ID to scroll to in the Results panel */
  focusResultId: string | null;
  setFocusResultId: (id: string | null) => void;
}

export interface SelectionSlice {
  selection: Selection;
  select: (selection: Selection) => void;
  clearSelection: () => void;
  neighborhoodFocus: NeighborhoodFocus | null;
  setNeighborhoodFocus: (focus: NeighborhoodFocus | null) => void;
}

export interface DashboardSlice {
  activeDashboardId: string | null;
  setActiveDashboardId: (id: string | null) => void;
  dashboardWidgetCount: number;
  setDashboardWidgetCount: (count: number) => void;
  dashboardFilters: Record<string, unknown>;
  setDashboardFilter: (key: string, value: unknown) => void;
  clearDashboardFilters: () => void;

  /**
   * Dashboard-scoped type-visibility cross-filter.
   *
   * Keyed by `dashboard_id` — every widget mounted inside the same
   * dashboard shares the same hidden-types list. Stored as `string[]`
   * rather than `Set<string>` because Zustand's `persist` middleware
   * serialises to JSON; `Set` round-trips as `{}` and loses contents.
   * The hook boundary does the `Set` conversion for ergonomic filter
   * APIs.
   *
   * A missing entry means "no types hidden on this dashboard" —
   * the widget-level hook treats absence and `[]` identically.
   */
  dashboardTypeFilters: Record<string, string[]>;
  toggleDashboardType: (dashboardId: string, type: string) => void;
  setDashboardTypeHidden: (dashboardId: string, type: string, hidden: boolean) => void;
  clearDashboardTypes: (dashboardId: string) => void;
}

export interface VerificationSlice {
  verifications: ElementVerification[];
  verificationsLoading: boolean;
  loadVerifications: (ontologyId: string) => Promise<void>;
  verifyElement: (ontologyId: string, elementId: string, elementKind: "node" | "edge" | "property", notes?: string) => Promise<void>;
  revokeVerification: (ontologyId: string, elementId: string) => Promise<void>;
  clearVerifications: () => void;
}

// Combined store type
export type AppStore = OntologySlice &
  ChatSlice &
  ProjectSlice &
  ChromeSlice &
  SelectionSlice &
  DashboardSlice &
  VerificationSlice;
