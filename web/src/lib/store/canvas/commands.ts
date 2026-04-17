"use client";

import { useCallback } from "react";
import { toast } from "sonner";

import { useAppStore } from "@/lib/store";
import { applyOntologyCommands } from "@/lib/api";
import { handleSchemaExport, type ExportFormat } from "@/lib/export-utils";
import type { OntologyIR } from "@/types/api";

interface CanvasCommandsOptions {
  setIsPaletteOpen: (v: boolean | ((v: boolean) => boolean)) => void;
  setIsExportOpen: (v: boolean | ((v: boolean) => boolean)) => void;
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
  const { setIsPaletteOpen, setIsExportOpen } = options;
  const ontology = useAppStore((s) => s.ontology);
  const select = useAppStore((s) => s.select);
  const clearSelection = useAppStore((s) => s.clearSelection);
  const applyCommand = useAppStore((s) => s.applyCommand);
  const setOntology = useAppStore((s) => s.setOntology);
  const setActiveProject = useAppStore((s) => s.setActiveProject);
  const setHighlightedBindings = useAppStore((s) => s.setHighlightedBindings);
  const setNeighborhoodFocus = useAppStore((s) => s.setNeighborhoodFocus);

  const handleSave = useCallback(async () => {
    const store = useAppStore.getState();
    if (!store.activeProject || store.commandStack.length === 0) return;
    try {
      const commands = store.commandStack.map((e) => e.command);
      const resp = await applyOntologyCommands(store.activeProject.id, {
        revision: store.activeProject.revision,
        commands,
      });
      setOntology(resp.project.ontology as OntologyIR);
      setActiveProject(resp.project);
      toast.success("Ontology saved");
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Failed to save");
    }
  }, [setOntology, setActiveProject]);

  const deleteSelected = useCallback(() => {
    const store = useAppStore.getState();
    const nodeId = store.selection.type === "node" ? store.selection.nodeId : null;
    const edgeId = store.selection.type === "edge" ? store.selection.edgeId : null;
    if (nodeId) {
      applyCommand({ op: "delete_node", node_id: nodeId });
      clearSelection();
      toast.success("Node deleted");
    } else if (edgeId) {
      applyCommand({ op: "delete_edge", edge_id: edgeId });
      clearSelection();
      toast.success("Edge deleted");
    }
  }, [applyCommand, clearSelection]);

  const selectAllNodes = useCallback(() => {
    if (ontology && ontology.node_types.length > 0) {
      select({ type: "node", nodeId: ontology.node_types[0].id });
    }
  }, [ontology, select]);

  const handleExport = useCallback(
    async (format: ExportFormat) => {
      if (!ontology) return;
      await handleSchemaExport(ontology, format);
    },
    [ontology],
  );

  const deselectAll = useCallback(() => {
    clearSelection();
    setHighlightedBindings(null);
    setIsPaletteOpen(false);
    setIsExportOpen(false);
    setNeighborhoodFocus(null);
  }, [clearSelection, setHighlightedBindings, setNeighborhoodFocus, setIsPaletteOpen, setIsExportOpen]);

  return { handleSave, deleteSelected, selectAllNodes, handleExport, deselectAll };
}
