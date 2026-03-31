"use client";

import { useCallback, useEffect } from "react";
import { useReactFlow } from "@xyflow/react";

import { useAppStore } from "@/lib/store";
import { usePrompt } from "@/components/ui/prompt-dialog";
import type { PaletteCommand } from "./command-palette";

interface KeyboardActions {
  handleSave: () => void;
  deleteSelected: () => void;
  runAutoLayout: () => void;
  selectAllNodes: () => void;
  deselectAll: () => void;
  exportPng: () => void;
  exportSvg: () => void;
  setIsPaletteOpen: (fn: (v: boolean) => boolean) => void;
}

/**
 * Global keyboard shortcuts and command palette command list.
 */
export function useCanvasKeyboard(actions: KeyboardActions) {
  const undoFn = useAppStore((s) => s.undo);
  const redoFn = useAppStore((s) => s.redo);
  const toggleExplorer = useAppStore((s) => s.toggleExplorer);
  const toggleInspector = useAppStore((s) => s.toggleInspector);
  const applyCommand = useAppStore((s) => s.applyCommand);
  const prompt = usePrompt();

  const { fitView, zoomIn, zoomOut } = useReactFlow();

  const {
    handleSave,
    deleteSelected,
    runAutoLayout,
    selectAllNodes,
    deselectAll,
    exportPng,
    exportSvg,
    setIsPaletteOpen,
  } = actions;

  // Global keyboard shortcuts
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      const meta = e.metaKey || e.ctrlKey;
      const tag = document.activeElement?.tagName;
      const inputFocused =
        tag === "INPUT" ||
        tag === "TEXTAREA" ||
        tag === "SELECT" ||
        (document.activeElement as HTMLElement)?.isContentEditable;

      if (meta && e.shiftKey && e.key.toLowerCase() === "p") {
        e.preventDefault();
        setIsPaletteOpen((v) => !v);
        return;
      }
      if (inputFocused) return;
      if (meta && !e.shiftKey && e.key === "z") { e.preventDefault(); undoFn(); return; }
      if (meta && e.shiftKey && e.key === "z") { e.preventDefault(); redoFn(); return; }
      if (meta && e.key === "s") { e.preventDefault(); handleSave(); return; }
      if (meta && e.key === "a") { e.preventDefault(); selectAllNodes(); return; }
      if (e.key === "Escape") { deselectAll(); return; }
      if (e.key === "Delete" || e.key === "Backspace") { e.preventDefault(); deleteSelected(); return; }
      if (meta && e.key === "0") { e.preventDefault(); fitView({ padding: 0.15, duration: 300 }); return; }
      if (meta && (e.key === "=" || e.key === "+")) { e.preventDefault(); zoomIn(); return; }
      if (meta && e.key === "-") { e.preventDefault(); zoomOut(); return; }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [undoFn, redoFn, handleSave, selectAllNodes, deselectAll, deleteSelected, fitView, zoomIn, zoomOut, setIsPaletteOpen]);

  // Command palette commands
  const paletteCommands = useCallback((): PaletteCommand[] => [
    { id: "undo", label: "Undo", shortcut: "\u2318Z", execute: undoFn },
    { id: "redo", label: "Redo", shortcut: "\u2318\u21e7Z", execute: redoFn },
    { id: "save", label: "Save Changes", shortcut: "\u2318S", execute: handleSave },
    { id: "auto-layout", label: "Auto Layout", execute: runAutoLayout },
    { id: "fit-view", label: "Fit View", execute: () => fitView({ padding: 0.15, duration: 300 }) },
    { id: "toggle-explorer", label: "Toggle Explorer", execute: toggleExplorer },
    { id: "toggle-inspector", label: "Toggle Inspector", execute: toggleInspector },
    { id: "export-png", label: "Export as PNG", execute: exportPng },
    { id: "export-svg", label: "Export as SVG", execute: exportSvg },
    { id: "ask-ontosyx", label: "Ask Ontosyx...", shortcut: "\u2318K", execute: () => {
      const store = useAppStore.getState();
      store.setDesignBottomTab("chat");
      if (!store.isBottomPanelOpen) store.toggleBottomPanel();
      // Focus the chat input after the panel renders
      setTimeout(() => {
        const chatInput = document.querySelector<HTMLTextAreaElement>("[data-chat-input]");
        chatInput?.focus();
      }, 100);
    }},
    { id: "add-node", label: "Add Node", execute: async () => {
      const l = await prompt({
        title: "Add Node",
        description: "Enter a label for the new node.",
        placeholder: "Node label",
        confirmLabel: "Add",
      });
      if (l?.trim()) {
        applyCommand({ op: "add_node", id: crypto.randomUUID(), label: l.trim() });
      }
    }},
    { id: "delete-selected", label: "Delete Selected", shortcut: "Delete", execute: deleteSelected },
    { id: "select-all", label: "Select All Nodes", shortcut: "\u2318A", execute: selectAllNodes },
    { id: "deselect-all", label: "Deselect All", shortcut: "Esc", execute: deselectAll },
    { id: "zoom-in", label: "Zoom In", shortcut: "\u2318+", execute: () => zoomIn() },
    { id: "zoom-out", label: "Zoom Out", shortcut: "\u2318-", execute: () => zoomOut() },
    { id: "zoom-fit", label: "Zoom to Fit", shortcut: "\u23180", execute: () => fitView({ padding: 0.15, duration: 300 }) },
  ], [undoFn, redoFn, handleSave, runAutoLayout, fitView, toggleExplorer, toggleInspector, exportPng, exportSvg, applyCommand, deleteSelected, selectAllNodes, deselectAll, zoomIn, zoomOut, prompt]);

  return { paletteCommands };
}
