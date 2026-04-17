# Canvas store slices

The Design workbench canvas was a single 577-line component that combined
ReactFlow wiring, keyboard shortcuts, context-menu logic, selection effects,
and high-level actions. Phase 5.7 split each concern into a dedicated hook
under `lib/store/canvas/`. The main `ontology-canvas.tsx` now orchestrates
these hooks and renders the canvas surface.

Each hook subscribes to the app store (`useAppStore`) with fine-grained
selectors so components re-render only when the slices they observe change.
The hooks are pure React hooks rather than separate Zustand stores because
they compose store state with ReactFlow context (`useReactFlow`), local UI
state (popover visibility), and dialog helpers — all of which benefit from
being evaluated inside the component tree.

## `commands.ts` — `useCanvasCommands`

High-level ontology actions.

- **Inputs**: `setIsPaletteOpen`, `setIsExportOpen` (the parent canvas owns
  these transient UI flags).
- **Returns**: `handleSave` (applies the pending command stack to the
  backend), `deleteSelected` (removes the selected node or edge),
  `selectAllNodes` (selects the first node — used as an accessibility hook),
  `handleExport` (runs a schema exporter), `deselectAll` (clears selection,
  highlights, neighborhood focus, and closes both popovers).

## `keyboard.ts` — `useCanvasKeyboard`

Global keyboard shortcuts and command-palette entries.

- **Shortcuts**: Cmd+Z / Cmd+Shift+Z (undo/redo), Cmd+S (save), Cmd+A (select
  all), Cmd+K is handled by `DesignLayout`, Cmd+Shift+P (palette),
  Delete/Backspace, Esc, Cmd+0/+/- (zoom).
- **Returns**: `paletteCommands` — memoizable factory producing the array of
  `PaletteCommand` entries consumed by `CommandPalette`.

## `context-menu.ts` — `useCanvasContextMenu`

Right-click menu state + items for nodes and edges.

- **Fields**: `contextMenu` (`{ type, id, x, y }` or null), updated by
  `handleNodeContextMenu` / `handleEdgeContextMenu`.
- **Computed**: `nodeContextMenuItems`, `edgeContextMenuItems` — menu entries
  for Inspect, Focus Neighborhood, Improve with AI, Rename, Change
  Cardinality (edges only), and Delete. All destructive actions route
  through the shared `useConfirm` dialog.
- **Actions**: `closeContextMenu`.

## `selection.ts` — `useCanvasSelection`

Visual-side effects of selection and neighborhood focus.

- **Inputs**: `ontology`, `setNodes`, `setEdges` (the parent ReactFlow
  setters).
- **Subscribed state**: `selection`, `neighborhoodFocus`.
- **Side effects**: pans/zooms the viewport to the selected element;
  patches ReactFlow `node.data.selected` / `node.data.dimmed` based on the
  current selection and neighborhood sets; installs an Escape handler that
  exits neighborhood focus mode.
- **Returns**: `{ selectedNodeId, selectedEdgeId, neighborhoodSets }` for
  callers that want to read the current selection without re-subscribing.

## `viewport.ts` — `useCanvasViewport`

Deterministic flow-element derivation.

- **Inputs**: `gaps` (quality report).
- **Returns**: `flowElements` (pre-built ReactFlow nodes+edges; null when no
  ontology is loaded) and `topologySignature` (string key that changes only
  when node/edge labels change — used by the layout engine to decide whether
  to re-run ELK).
- **Side effects**: one-shot auto-grouping for large ontologies (runs once
  per topology when no groups exist).
