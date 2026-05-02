"use client";

import { useCallback, useMemo } from "react";
import { useTranslations } from "next-intl";
import { toast } from "sonner";
import type { NodeMouseHandler, EdgeMouseHandler } from "@xyflow/react";

import { useAppStore } from "@/lib/store";
import { useConfirm } from "@/components/providers/confirm-provider";
import { usePrompt } from "@/components/providers/prompt-provider";
import { editProject } from "@/lib/api";
import type { ContextMenuItem } from "@/components/workbench/canvas/context-menu";
import type { UseGraphContextMenuResult } from "@/hooks/use-graph-context-menu";
import type { OntologyIR, OntologyCommand } from "@/types/api";
import { arr } from "@/lib/ir-collections";

// ---------------------------------------------------------------------------
// useOntologyContextMenu — ontology-aware right-click menu contributions
// ---------------------------------------------------------------------------
//
// Splits cleanly along the primitive / policy boundary the canvas uses
// elsewhere:
//
//   • `useGraphContextMenu` (in lib/) owns the *state* — open, close,
//     target, position — and is shared with QueryCanvas / ExploreCanvas
//     / future surfaces.
//   • This hook owns the *policy* — the domain-specific menu items
//     (Inspect, Focus Neighborhood, Improve with AI, Rename, Delete)
//     that only make sense for the ontology editor — plus the right-click
//     handlers that push a target into the shared state.
//
// Callers construct the shared state via `useGraphContextMenu()` and
// thread it in here. Rendering the `<ContextMenu/>` component stays in
// the canvas itself so the overlay layer is colocated with the rest of
// the canvas chrome.

/** User-facing copy threaded in from the React layer (localised). */
interface ImproveWithAiCopy {
  analyzing: string;
  noImprovements: string;
  applied: string;
  undoHint: string;
  failed: string;
  unknownError: string;
}

/**
 * LLM prompt templates. These are LLM input — kept in English by
 * design, never localised. Separated from `ImproveWithAiCopy` so the
 * type's contract is clean: i18n strings on one side, model input on
 * the other.
 */
const AI_IMPROVE_PROMPTS = {
  node: 'Suggest improvements for the "{label}" node: better description, additional useful properties, and any missing constraints or relationships.',
  edge: 'Suggest improvements for the "{label}" edge: better description, additional useful properties, and correct cardinality.',
} as const;

async function improveWithAi(
  entityType: "node" | "edge",
  entityLabel: string,
  projectId: string,
  revision: number,
  applyCommand: (cmd: OntologyCommand) => void,
  copy: ImproveWithAiCopy,
) {
  const loading = toast.loading(copy.analyzing.replace("{label}", entityLabel));
  try {
    const userRequest = AI_IMPROVE_PROMPTS[entityType].replace(
      "{label}",
      entityLabel,
    );
    const resp = await editProject(projectId, {
      revision,
      user_request: userRequest,
      dry_run: true,
    });
    toast.dismiss(loading);
    if (resp.commands.length === 0) {
      toast.info(copy.noImprovements, { description: resp.explanation });
    } else {
      for (const cmd of resp.commands) {
        applyCommand(cmd);
      }
      toast.success(
        copy.applied.replace("{count}", String(resp.commands.length)),
        { description: copy.undoHint },
      );
    }
  } catch (err) {
    toast.dismiss(loading);
    toast.error(copy.failed, {
      description: err instanceof Error ? err.message : copy.unknownError,
    });
  }
}

export interface UseOntologyContextMenuResult {
  handleNodeContextMenu: NodeMouseHandler;
  handleEdgeContextMenu: EdgeMouseHandler;
  nodeContextMenuItems: ContextMenuItem[];
  edgeContextMenuItems: ContextMenuItem[];
}

export function useOntologyContextMenu(
  ontology: OntologyIR | null,
  contextMenu: UseGraphContextMenuResult,
): UseOntologyContextMenuResult {
  const t = useTranslations("workbench.contextMenu");
  const tInspector = useTranslations("inspector.toast");
  const select = useAppStore((s) => s.select);
  const clearSelection = useAppStore((s) => s.clearSelection);
  const applyCommand = useAppStore((s) => s.applyCommand);
  const toggleInspector = useAppStore((s) => s.toggleInspector);
  const setNeighborhoodFocus = useAppStore((s) => s.setNeighborhoodFocus);

  const confirm = useConfirm();
  const prompt = usePrompt();

  const aiCopy: ImproveWithAiCopy = useMemo(
    () => ({
      analyzing: t("ai.analyzing"),
      noImprovements: tInspector("noImprovements"),
      applied: t("ai.applied"),
      undoHint: t("ai.undoHint"),
      failed: tInspector("improvementFailed"),
      unknownError: t("ai.unknownError"),
    }),
    [t, tInspector],
  );

  const handleNodeContextMenu: NodeMouseHandler = useCallback(
    (event, node) => {
      if (node.type === "group") return;
      select({ type: "node", nodeId: node.id });
      contextMenu.open(event, { type: "node", id: node.id });
    },
    [contextMenu, select],
  );

  const handleEdgeContextMenu: EdgeMouseHandler = useCallback(
    (event, edge) => {
      select({ type: "edge", edgeId: edge.id });
      contextMenu.open(event, { type: "edge", id: edge.id });
    },
    [contextMenu, select],
  );

  const target = contextMenu.state?.target ?? null;

  const nodeContextMenuItems = useMemo((): ContextMenuItem[] => {
    if (!target || target.type !== "node" || !ontology) return [];
    const nodeId = target.id;
    const nodeDef = arr(ontology.node_types).find((n) => n.id === nodeId);
    if (!nodeDef) return [];
    const connectedEdges = arr(ontology.edge_types).filter(
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
          await improveWithAi("node", nodeDef.label, activeProject.id, activeProject.revision, applyCommand, aiCopy);
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
  }, [target, ontology, select, clearSelection, applyCommand, toggleInspector, setNeighborhoodFocus, confirm, prompt, aiCopy]);

  const edgeContextMenuItems = useMemo((): ContextMenuItem[] => {
    if (!target || target.type !== "edge" || !ontology) return [];
    const edgeId = target.id;
    const edgeDef = arr(ontology.edge_types).find((e) => e.id === edgeId);
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
          await improveWithAi("edge", edgeDef.label, project.id, project.revision, applyCommand, aiCopy);
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
  }, [target, ontology, select, clearSelection, applyCommand, toggleInspector, confirm, prompt, aiCopy]);

  return {
    handleNodeContextMenu,
    handleEdgeContextMenu,
    nodeContextMenuItems,
    edgeContextMenuItems,
  };
}
