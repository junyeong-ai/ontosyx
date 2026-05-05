"use client";

// `useCanvasKeyboardMovement` — keyboard-only node positioning.
//
// Without this hook, the only way to reposition a node on the
// design canvas is mouse drag — a hard a11y blocker for keyboard
// users. Pattern matches Figma's selected-layer move semantics:
//
//   ArrowKey       — 1px nudge (precise alignment)
//   Shift+Arrow    — 10px nudge (visible movement)
//   Mod+Arrow      — 100px nudge (cross-canvas reposition)
//
// Multi-select moves every selected node by the same delta — so
// a Shift+drag-selected group of nodes can be repositioned as a
// rigid block. Edges are skipped (their position is derived from
// endpoints) and group nodes are skipped (they're layout
// containers, not targets).
//
// The hook gates on:
//   * `enabled: () => selection has at least one node` — Arrow keys
//     pass through to other handlers when nothing is selected, so
//     scrolling lists / reading documentation still works.
//   * `fireInTypingTarget: false` (registry default) — typing in
//     a search input never moves the canvas.

import { useCallback } from "react";
import type { Dispatch, SetStateAction } from "react";
import type { Node } from "@xyflow/react";

import { selectStateSelection, useAppStore } from "@/lib/store";
import { useShortcut } from "@/lib/shortcuts";

interface KeyboardMovementOptions {
  setNodes: Dispatch<SetStateAction<Node[]>>;
}

/** Delta in flow coordinates per arrow press for each modifier. */
function stepFor(modifier: "none" | "shift" | "mod"): number {
  if (modifier === "mod") return 100;
  if (modifier === "shift") return 10;
  return 1;
}

export function useCanvasKeyboardMovement({
  setNodes,
}: KeyboardMovementOptions) {
  const move = useCallback(
    (dx: number, dy: number) => {
      const selectedNodeIds = new Set(
        useAppStore
          .getState()
          .selection.refs.filter((r) => r.kind === "node")
          .map((r) => r.id),
      );
      if (selectedNodeIds.size === 0) return;
      setNodes((prev) =>
        prev.map((n) => {
          if (n.type === "group") return n;
          if (!selectedNodeIds.has(n.id)) return n;
          return {
            ...n,
            position: {
              x: n.position.x + dx,
              y: n.position.y + dy,
            },
          };
        }),
      );
    },
    [setNodes],
  );

  const hasNodeSelection = useCallback(
    () =>
      useAppStore
        .getState()
        .selection.refs.some((r) => r.kind === "node"),
    [],
  );

  // One spec per direction × modifier so the registry's collision
  // detection sees them as distinct rather than as four overlapping
  // multi-key shortcuts. The `enabled` predicate gates on the live
  // selection so an empty canvas doesn't hijack arrow keys from
  // surrounding scroll containers.
  useShortcut({
    id: "canvas.move.left",
    keys: ["ArrowLeft", "shift+ArrowLeft", "mod+ArrowLeft"],
    group: "keyboardShortcuts.sections.canvas",
    description: "keyboardShortcuts.shortcuts.moveLeft",
    priority: 5,
    enabled: hasNodeSelection,
    handler: (e) => {
      e.preventDefault();
      const mod = e.metaKey || e.ctrlKey ? "mod" : e.shiftKey ? "shift" : "none";
      move(-stepFor(mod), 0);
    },
  });
  useShortcut({
    id: "canvas.move.right",
    keys: ["ArrowRight", "shift+ArrowRight", "mod+ArrowRight"],
    group: "keyboardShortcuts.sections.canvas",
    description: "keyboardShortcuts.shortcuts.moveRight",
    priority: 5,
    enabled: hasNodeSelection,
    handler: (e) => {
      e.preventDefault();
      const mod = e.metaKey || e.ctrlKey ? "mod" : e.shiftKey ? "shift" : "none";
      move(stepFor(mod), 0);
    },
  });
  useShortcut({
    id: "canvas.move.up",
    keys: ["ArrowUp", "shift+ArrowUp", "mod+ArrowUp"],
    group: "keyboardShortcuts.sections.canvas",
    description: "keyboardShortcuts.shortcuts.moveUp",
    priority: 5,
    enabled: hasNodeSelection,
    handler: (e) => {
      e.preventDefault();
      const mod = e.metaKey || e.ctrlKey ? "mod" : e.shiftKey ? "shift" : "none";
      move(0, -stepFor(mod));
    },
  });
  useShortcut({
    id: "canvas.move.down",
    keys: ["ArrowDown", "shift+ArrowDown", "mod+ArrowDown"],
    group: "keyboardShortcuts.sections.canvas",
    description: "keyboardShortcuts.shortcuts.moveDown",
    priority: 5,
    enabled: hasNodeSelection,
    handler: (e) => {
      e.preventDefault();
      const mod = e.metaKey || e.ctrlKey ? "mod" : e.shiftKey ? "shift" : "none";
      move(0, stepFor(mod));
    },
  });

  // Read selection for testability — consumers can subscribe to
  // verify the hook re-evaluates when selection changes.
  const selection = useAppStore(selectStateSelection);
  void selection;
}
