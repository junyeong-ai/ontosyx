"use client";

import { useEffect } from "react";
import {
  useGraphContextMenu,
  type GraphContextMenuTarget,
  type UseGraphContextMenuResult,
} from "./use-graph-context-menu";

// ---------------------------------------------------------------------------
// useGraphInteractions — shared interaction policy for graph canvases
// ---------------------------------------------------------------------------
//
// Composes three concerns that every graph surface has to implement:
//
//   1. Context menu state (position + target) — via `useGraphContextMenu`.
//   2. Keyboard deletion of the selected node / edge. The surface supplies
//      the remove callbacks; the hook installs a capture-phase listener
//      so `Delete` / `Backspace` in a canvas-focused region works without
//      the surface having to wire a key handler on every node.
//   3. An "escape clears selection" shortcut — matches our other workbench
//      surfaces (OntologyCanvas, CommandBar) so muscle memory transfers.
//
// The hook is UI-framework agnostic: callers decide what "selected" means.
// It just invokes `onRemoveNode` / `onRemoveEdge` with the id carried in
// `selectedTarget`. When `selectedTarget` is `null` the keyboard shortcuts
// become no-ops.

export interface UseGraphInteractionsOptions {
  /** What the canvas currently considers selected (matches context-menu target shape). */
  selectedTarget: GraphContextMenuTarget | null;
  /** Clear canvas selection state. Called on Escape. */
  onClearSelection: () => void;
  /** Remove a node by id. Called on Delete/Backspace when a node is selected. */
  onRemoveNode: (id: string) => void;
  /** Remove an edge by id. Called on Delete/Backspace when an edge is selected. */
  onRemoveEdge: (id: string) => void;
  /**
   * Optional guard — e.g. don't swallow Delete while an input is focused
   * inside the canvas region. Defaults to "always active".
   */
  enabled?: boolean;
}

export interface UseGraphInteractionsResult {
  contextMenu: UseGraphContextMenuResult;
}

export function useGraphInteractions(
  options: UseGraphInteractionsOptions,
): UseGraphInteractionsResult {
  const { selectedTarget, onClearSelection, onRemoveNode, onRemoveEdge } = options;
  const enabled = options.enabled ?? true;
  const contextMenu = useGraphContextMenu();

  useEffect(() => {
    if (!enabled) return;

    const onKey = (event: KeyboardEvent) => {
      // Don't hijack shortcuts while the user is typing in a form field
      // anywhere on the page. The canvas doesn't own the whole document.
      const target = event.target as HTMLElement | null;
      if (
        target &&
        (target.tagName === "INPUT" ||
          target.tagName === "TEXTAREA" ||
          target.isContentEditable)
      ) {
        return;
      }

      if (event.key === "Escape") {
        if (contextMenu.state) {
          contextMenu.close();
          event.preventDefault();
        } else if (selectedTarget) {
          onClearSelection();
          event.preventDefault();
        }
        return;
      }

      if (event.key === "Delete" || event.key === "Backspace") {
        if (!selectedTarget) return;
        event.preventDefault();
        if (selectedTarget.type === "node") {
          onRemoveNode(selectedTarget.id);
        } else {
          onRemoveEdge(selectedTarget.id);
        }
        onClearSelection();
      }
    };

    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [enabled, selectedTarget, onClearSelection, onRemoveNode, onRemoveEdge, contextMenu]);

  return { contextMenu };
}
