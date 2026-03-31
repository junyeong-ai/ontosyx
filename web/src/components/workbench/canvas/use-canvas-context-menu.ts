"use client";

import { useCallback, useMemo, useState } from "react";
import type { NodeMouseHandler, EdgeMouseHandler } from "@xyflow/react";

import { useAppStore } from "@/lib/store";
import type { ContextMenuState, ContextMenuItem } from "./context-menu";
import { toast } from "sonner";
import { useConfirm } from "@/components/ui/confirm-dialog";
import { usePrompt } from "@/components/ui/prompt-dialog";
import type { OntologyIR, OntologyCommand } from "@/types/api";
import { editProject } from "@/lib/api";

async function improveWithAi(
  entityType: "node" | "edge",
  entityLabel: string,
  projectId: string,
  revision: number,
  applyCommand: (cmd: OntologyCommand) => void,
) {
  const loading = toast.loading(`Analyzing "${entityLabel}"...`);
  try {
    const resp = await editProject(projectId, {
      revision,
      user_request: entityType === "node"
        ? `Suggest improvements for the "${entityLabel}" node: better description, additional useful properties, and any missing constraints or relationships.`
        : `Suggest improvements for the "${entityLabel}" edge: better description, additional useful properties, and correct cardinality.`,
      dry_run: true,
    });
    toast.dismiss(loading);
    if (resp.commands.length === 0) {
      toast.info("No improvements suggested", { description: resp.explanation });
    } else {
      for (const cmd of resp.commands) {
        applyCommand(cmd);
      }
      toast.success(`${resp.commands.length} improvement${resp.commands.length > 1 ? "s" : ""} applied`, {
        description: "Use Ctrl+Z to undo",
      });
    }
  } catch (err) {
    toast.dismiss(loading);
    toast.error("AI improvement failed", { description: err instanceof Error ? err.message : "Unknown error" });
  }
}

/**
 * Context menu state, items, and event handlers for nodes and edges.
 */
export function useCanvasContextMenu(ontology: OntologyIR | null) {
  const select = useAppStore((s) => s.select);
  const clearSelection = useAppStore((s) => s.clearSelection);
  const applyCommand = useAppStore((s) => s.applyCommand);
  const toggleInspector = useAppStore((s) => s.toggleInspector);
  const setNeighborhoodFocus = useAppStore((s) => s.setNeighborhoodFocus);

  const confirm = useConfirm();
  const prompt = usePrompt();

  const [contextMenu, setContextMenu] = useState<ContextMenuState | null>(null);

  const handleNodeContextMenu: NodeMouseHandler = useCallback(
    (event, node) => {
      event.preventDefault();
      if (node.type === "group") return;
      select({ type: "node", nodeId: node.id });
      setContextMenu({
        type: "node",
        id: node.id,
        x: event.clientX,
        y: event.clientY,
      });
    },
    [select],
  );

  const handleEdgeContextMenu: EdgeMouseHandler = useCallback(
    (event, edge) => {
      event.preventDefault();
      select({ type: "edge", edgeId: edge.id });
      setContextMenu({
        type: "edge",
        id: edge.id,
        x: event.clientX,
        y: event.clientY,
      });
    },
    [select],
  );

  const nodeContextMenuItems = useMemo((): ContextMenuItem[] => {
    if (!contextMenu || contextMenu.type !== "node" || !ontology) return [];
    const nodeId = contextMenu.id;
    const nodeDef = ontology.node_types.find((n) => n.id === nodeId);
    if (!nodeDef) return [];
    const connectedEdges = ontology.edge_types.filter(
      (e) => e.source_node_id === nodeId || e.target_node_id === nodeId,
    );
    const activeProject = useAppStore.getState().activeProject;
    return [
      { label: "Inspect", onClick: () => { select({ type: "node", nodeId }); if (!useAppStore.getState().isInspectorOpen) toggleInspector(); } },
      { label: "Focus Neighborhood", onClick: () => setNeighborhoodFocus({ nodeId, depth: 1 }) },
      {
        label: "Improve with AI",
        disabled: !activeProject,
        onClick: async () => {
          if (!activeProject) return;
          select({ type: "node", nodeId });
          if (!useAppStore.getState().isInspectorOpen) toggleInspector();
          await improveWithAi("node", nodeDef.label, activeProject.id, activeProject.revision, applyCommand);
        },
      },
      { label: "Add Property", onClick: () => { select({ type: "node", nodeId }); if (!useAppStore.getState().isInspectorOpen) toggleInspector(); } },
      {
        label: "Rename",
        onClick: async () => {
          const v = await prompt({
            title: "Rename Node",
            description: `Enter a new label for "${nodeDef.label}".`,
            defaultValue: nodeDef.label,
            confirmLabel: "Rename",
          });
          if (v?.trim() && v.trim() !== nodeDef.label) {
            applyCommand({ op: "rename_node", node_id: nodeId, new_label: v.trim() });
          }
        },
      },
      {
        label: connectedEdges.length > 0
          ? `Delete Node (${connectedEdges.length} edge${connectedEdges.length !== 1 ? "s" : ""})`
          : "Delete Node",
        danger: true,
        onClick: async () => {
          if (connectedEdges.length > 0) {
            const confirmed = await confirm({
              title: "Delete Node",
              description: `Delete "${nodeDef.label}" and ${connectedEdges.length} connected edge(s)?`,
              confirmLabel: "Delete",
              variant: "danger",
            });
            if (!confirmed) return;
          }
          applyCommand({ op: "delete_node", node_id: nodeId });
          clearSelection();
          toast.success(`Node "${nodeDef.label}" deleted`);
        },
      },
    ];
  }, [contextMenu, ontology, select, clearSelection, applyCommand, toggleInspector, setNeighborhoodFocus, confirm, prompt]);

  const edgeContextMenuItems = useMemo((): ContextMenuItem[] => {
    if (!contextMenu || contextMenu.type !== "edge" || !ontology) return [];
    const edgeId = contextMenu.id;
    const edgeDef = ontology.edge_types.find((e) => e.id === edgeId);
    if (!edgeDef) return [];
    const project = useAppStore.getState().activeProject;
    return [
      { label: "Inspect", onClick: () => { select({ type: "edge", edgeId }); if (!useAppStore.getState().isInspectorOpen) toggleInspector(); } },
      {
        label: "Improve with AI",
        disabled: !project,
        onClick: async () => {
          if (!project) return;
          select({ type: "edge", edgeId });
          if (!useAppStore.getState().isInspectorOpen) toggleInspector();
          await improveWithAi("edge", edgeDef.label, project.id, project.revision, applyCommand);
        },
      },
      {
        label: "Rename",
        onClick: async () => {
          const v = await prompt({
            title: "Rename Edge",
            description: `Enter a new label for "${edgeDef.label}".`,
            defaultValue: edgeDef.label,
            confirmLabel: "Rename",
          });
          if (v?.trim() && v.trim() !== edgeDef.label) {
            applyCommand({ op: "rename_edge", edge_id: edgeId, new_label: v.trim() });
          }
        },
      },
      {
        label: "Change Cardinality",
        submenu: (["one_to_one", "one_to_many", "many_to_one", "many_to_many"] as const).map((c) => ({
          label: c.replace(/_/g, " "),
          onClick: () => {
            applyCommand({ op: "update_edge_cardinality", edge_id: edgeId, cardinality: c });
            toast.success(`Cardinality: ${c.replace(/_/g, " ")}`);
          },
        })),
      },
      {
        label: "Delete Edge",
        danger: true,
        onClick: async () => {
          const confirmed = await confirm({
            title: "Delete Edge",
            description: `Delete edge "${edgeDef.label}"?`,
            confirmLabel: "Delete",
            variant: "danger",
          });
          if (!confirmed) return;
          applyCommand({ op: "delete_edge", edge_id: edgeId });
          clearSelection();
          toast.success(`Edge "${edgeDef.label}" deleted`);
        },
      },
    ];
  }, [contextMenu, ontology, select, clearSelection, applyCommand, toggleInspector, confirm, prompt]);

  const closeContextMenu = useCallback(() => setContextMenu(null), []);

  return {
    contextMenu,
    closeContextMenu,
    handleNodeContextMenu,
    handleEdgeContextMenu,
    nodeContextMenuItems,
    edgeContextMenuItems,
  };
}
