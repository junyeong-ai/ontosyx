"use client";

import { useCallback } from "react";
import { toast } from "@/components/ui/toast";

import { useAppStore } from "@/lib/store";
import { applyOntologyCommands } from "@/lib/api";
import {
  handleSchemaExport,
  type ExportFormat,
  type SchemaExportToastCopy,
} from "@/lib/export-utils";

export interface CanvasCommandsToastCopy {
  saved: string;
  saveFailed: string;
  nodeDeleted: string;
  edgeDeleted: string;
}

interface CanvasCommandsOptions {
  setIsExportOpen: (v: boolean | ((v: boolean) => boolean)) => void;
  exportToastCopy: SchemaExportToastCopy;
  toastCopy: CanvasCommandsToastCopy;
}

export interface CanvasCommands {
  handleSave: () => Promise<void>;
  deleteSelected: () => void;
  selectAllNodes: () => void;
  handleExport: (format: ExportFormat) => Promise<void>;
  deselectAll: () => void;
}

/**
 * High-level canvas commands — save, delete, select-all, export, deselect.
 *
 * These are user-facing actions that mutate ontology state or transient UI
 * state (popover visibility). They are intentionally hook-based rather than
 * Zustand store state because they compose ontology actions, toast feedback,
 * and UI-local setters owned by the canvas component.
 */
export function useCanvasCommands(options: CanvasCommandsOptions): CanvasCommands {
  const { setIsExportOpen, exportToastCopy, toastCopy } = options;
  const ontology = useAppStore((s) => s.ontology);
  const clearSelection = useAppStore((s) => s.clearSelection);
  const applyCommand = useAppStore((s) => s.applyCommand);
  const applyOntologyDraftSnapshot = useAppStore((s) => s.applyOntologyDraftSnapshot);
  const setHighlightedBindings = useAppStore((s) => s.setHighlightedBindings);
  const setNeighborhoodFocus = useAppStore((s) => s.setNeighborhoodFocus);

  const handleSave = useCallback(async () => {
    const store = useAppStore.getState();
    if (!store.activeOntologyDraft || store.commandStack.length === 0) return;
    try {
      const commands = store.commandStack.map((e) => e.command);
      const resp = await applyOntologyCommands(store.activeOntologyDraft.id, {
        revision: store.activeOntologyDraft.revision,
        commands,
      });
      // Server canonical replaces local state + clears command stack
      // atomically through `applyOntologyDraftSnapshot`.
      applyOntologyDraftSnapshot(resp.project);
      toast.success(toastCopy.saved);
    } catch (err) {
      toast.error(err instanceof Error ? err.message : toastCopy.saveFailed);
    }
  }, [applyOntologyDraftSnapshot, toastCopy]);

  const deleteSelected = useCallback(() => {
    const store = useAppStore.getState();
    // Bulk delete: every selected ref of node/edge kind drops out
    // through the same command stack as a single-target delete, so
    // multi-select honours undo/redo. Node deletes go first because
    // an edge whose endpoint is being deleted on the same turn is
    // already implied by `delete_node`.
    const nodeIds = store.selection.refs
      .filter((r) => r.kind === "node")
      .map((r) => r.id);
    const edgeIds = store.selection.refs
      .filter((r) => r.kind === "edge")
      .map((r) => r.id);
    if (nodeIds.length === 0 && edgeIds.length === 0) return;
    for (const id of nodeIds) {
      applyCommand({ op: "delete_node", node_id: id });
    }
    for (const id of edgeIds) {
      applyCommand({ op: "delete_edge", edge_id: id });
    }
    clearSelection();
    if (nodeIds.length > 0) toast.success(toastCopy.nodeDeleted);
    else if (edgeIds.length > 0) toast.success(toastCopy.edgeDeleted);
  }, [applyCommand, clearSelection, toastCopy]);

  const selectAllNodes = useCallback(() => {
    if (!ontology) return;
    const refs = ontology.node_types.map((n) => ({
      kind: "node" as const,
      id: n.id,
    }));
    if (refs.length > 0) {
      useAppStore.getState().selectMany(refs);
    }
  }, [ontology]);

  const handleExport = useCallback(
    async (format: ExportFormat) => {
      if (!ontology) return;
      await handleSchemaExport(ontology, format, exportToastCopy);
    },
    [ontology, exportToastCopy],
  );

  const deselectAll = useCallback(() => {
    clearSelection();
    setHighlightedBindings(null);
    setIsExportOpen(false);
    setNeighborhoodFocus(null);
  }, [clearSelection, setHighlightedBindings, setNeighborhoodFocus, setIsExportOpen]);

  return { handleSave, deleteSelected, selectAllNodes, handleExport, deselectAll };
}
