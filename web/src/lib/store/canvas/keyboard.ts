"use client";

import { useEffect } from "react";
import { useReactFlow } from "@xyflow/react";

import { useAppStore } from "@/lib/store";

interface KeyboardActions {
  handleSave: () => void;
  deleteSelected: () => void;
  selectAllNodes: () => void;
  deselectAll: () => void;
}

/**
 * Direct-keyboard shortcuts active while the canvas is mounted.
 * Discrete actions (Cmd+S save, Cmd+Z undo, Cmd+A select-all,
 * Esc deselect, Delete remove, Cmd+0/+/- zoom) live here so power
 * users don't have to round-trip through the palette. Discoverable
 * variants of every action are also registered into the unified
 * command registry by `CanvasCommandSource`.
 */
export function useCanvasKeyboard(actions: KeyboardActions) {
  const undoFn = useAppStore((s) => s.undo);
  const redoFn = useAppStore((s) => s.redo);

  const { fitView, zoomIn, zoomOut } = useReactFlow();

  const {
    handleSave,
    deleteSelected,
    selectAllNodes,
    deselectAll,
  } = actions;

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      const meta = e.metaKey || e.ctrlKey;
      const tag = document.activeElement?.tagName;
      const inputFocused =
        tag === "INPUT" ||
        tag === "TEXTAREA" ||
        tag === "SELECT" ||
        (document.activeElement as HTMLElement)?.isContentEditable;

      if (inputFocused) return;
      if (meta && !e.shiftKey && e.key === "z") {
        e.preventDefault();
        undoFn();
        return;
      }
      if (meta && e.shiftKey && e.key === "z") {
        e.preventDefault();
        redoFn();
        return;
      }
      if (meta && e.key === "s") {
        e.preventDefault();
        handleSave();
        return;
      }
      if (meta && e.key === "a") {
        e.preventDefault();
        selectAllNodes();
        return;
      }
      if (e.key === "Escape") {
        deselectAll();
        return;
      }
      if (e.key === "Delete" || e.key === "Backspace") {
        e.preventDefault();
        deleteSelected();
        return;
      }
      if (meta && e.key === "0") {
        e.preventDefault();
        fitView({ padding: 0.15, duration: 300 });
        return;
      }
      if (meta && (e.key === "=" || e.key === "+")) {
        e.preventDefault();
        zoomIn();
        return;
      }
      if (meta && e.key === "-") {
        e.preventDefault();
        zoomOut();
        return;
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [
    undoFn,
    redoFn,
    handleSave,
    selectAllNodes,
    deselectAll,
    deleteSelected,
    fitView,
    zoomIn,
    zoomOut,
  ]);
}
