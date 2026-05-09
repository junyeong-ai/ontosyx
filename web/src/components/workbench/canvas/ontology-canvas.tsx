"use client";

import { useCallback, useMemo, useState } from "react";
import { useTranslations } from "next-intl";
import {
  useNodesState,
  useEdgesState,
  ReactFlowProvider,
  type Node,
  type Edge,
  type NodeMouseHandler,
  type EdgeMouseHandler,
  type OnSelectionChangeFunc,
} from "@xyflow/react";

import type { SelectionRef } from "@/lib/store";

import { CommandBar } from "./command-bar";
import { DiffOverlayBar } from "./diff-overlay-bar";
import { VersionDiffBar } from "./version-diff-bar";
import { ContextMenu } from "./context-menu";
import { CanvasCommandSource } from "./canvas-command-source";
import { NeighborhoodToolbar } from "./neighborhood-toolbar";
import { exportCanvasImage } from "./canvas-helpers";
import { useCanvasLayout } from "./use-canvas-layout";
import type { ElkLayoutPreset } from "./elk-layout";
import { CanvasSkeleton } from "./canvas-skeleton";
import { CanvasEmptyState, CanvasZeroNodesState } from "./canvas-empty-state";
import { arr } from "@/lib/ir-collections";
import { CanvasToolbar } from "./canvas-toolbar";
import { CanvasFlow } from "./canvas-flow";
import { useCanvasState } from "./use-canvas-state";
import { useOntologyContextMenu } from "./use-ontology-context-menu";
import { useCanvasCommands } from "@/lib/store/canvas/commands";
import { useCanvasKeyboard } from "@/lib/store/canvas/keyboard";
import { useCanvasKeyboardMovement } from "@/lib/store/canvas/keyboard-movement";
import { useCanvasSelection } from "@/lib/store/canvas/selection";
import { useCanvasViewport } from "@/lib/store/canvas/viewport";
import { useGraphContextMenu } from "@/hooks/use-graph-context-menu";
import type { QualityGap } from "@/types/api";

function CanvasInner({ gaps }: { gaps: QualityGap[] }) {
  const tModeActions = useTranslations("chrome.modeActions");
  const tInspector = useTranslations("inspector.toast");
  const tImage = useTranslations("workbench.canvas.toolbar.image");
  const {
    ontology,
    selectOne,
    toggleSelection,
    selectMany,
    clearSelection,
    setHighlightedBindings,
    setNeighborhoodFocus,
  } = useCanvasState();
  const imageCopy = useMemo(
    () => ({
      nothingToExport: tImage("nothingToExport"),
      exported: tImage("exported"),
      failed: tImage("failed"),
    }),
    [tImage],
  );

  const [nodes, setNodes, onNodesChange] = useNodesState<Node>([]);
  const [edges, setEdges, onEdgesChange] = useEdgesState<Edge>([]);

  const [isExportOpen, setIsExportOpen] = useState(false);
  const [layout, setLayout] = useState<ElkLayoutPreset>("layered");

  const { flowElements, topologySignature } = useCanvasViewport(gaps);

  useCanvasSelection({ ontology, setNodes, setEdges });
  useCanvasKeyboardMovement({ setNodes });

  const { onNodeDragStop, runAutoLayout, layoutReady } = useCanvasLayout(
    flowElements,
    topologySignature,
    setNodes,
    setEdges,
    layout,
  );

  const { handleSave, deleteSelected, selectAllNodes, handleExport, deselectAll } = useCanvasCommands({
    setIsExportOpen,
    exportToastCopy: { failedTitle: tModeActions("exportFailed") },
    toastCopy: {
      saved: tInspector("saved"),
      saveFailed: tInspector("saveFailed"),
      nodeDeleted: tInspector("nodeDeleted"),
      edgeDeleted: tInspector("edgeDeleted"),
    },
  });

  useCanvasKeyboard({
    handleSave,
    deleteSelected,
    selectAllNodes,
    deselectAll,
  });
  const canvasCommandActions = useMemo(
    () => ({
      handleSave,
      deleteSelected,
      runAutoLayout: () => runAutoLayout(nodes, edges),
      selectAllNodes,
      deselectAll,
      exportPng: () =>
        exportCanvasImage(nodes, "png", ontology?.name ?? "ontology", imageCopy),
      exportSvg: () =>
        exportCanvasImage(nodes, "svg", ontology?.name ?? "ontology", imageCopy),
    }),
    [
      handleSave,
      deleteSelected,
      runAutoLayout,
      selectAllNodes,
      deselectAll,
      nodes,
      edges,
      ontology?.name,
      imageCopy,
    ],
  );

  const contextMenu = useGraphContextMenu();
  const {
    handleNodeContextMenu,
    handleEdgeContextMenu,
    nodeContextMenuItems,
    edgeContextMenuItems,
  } = useOntologyContextMenu(ontology, contextMenu);

  // Modifier-aware click → store. Cmd / Ctrl toggles add/remove
  // (mac Cmd, win/linux Ctrl); a plain click replaces the selection.
  // Shift + drag is owned by ReactFlow's lasso (`selectionKeyCode="Shift"`)
  // — we never reach here for box selections.
  const onNodeClick: NodeMouseHandler = useCallback(
    (event, node) => {
      contextMenu.close();
      if (node.type === "group") return;
      const ref = { kind: "node" as const, id: node.id };
      if (event.metaKey || event.ctrlKey) toggleSelection(ref);
      else selectOne(ref);
    },
    [selectOne, toggleSelection, contextMenu],
  );

  const onNodeDoubleClick: NodeMouseHandler = useCallback(
    (_event, node) => {
      if (node.type === "group") return;
      setNeighborhoodFocus({ nodeId: node.id, depth: 1 });
    },
    [setNeighborhoodFocus],
  );

  const onEdgeClick: EdgeMouseHandler = useCallback(
    (event, edge) => {
      contextMenu.close();
      const ref = { kind: "edge" as const, id: edge.id };
      if (event.metaKey || event.ctrlKey) toggleSelection(ref);
      else selectOne(ref);
    },
    [selectOne, toggleSelection, contextMenu],
  );

  const onPaneClick = useCallback(() => {
    contextMenu.close();
    clearSelection();
    setHighlightedBindings(null);
  }, [clearSelection, setHighlightedBindings, contextMenu]);

  // Lasso (shift-drag) result → store. ReactFlow tracks its own
  // `node.selected` flag during the drag; on completion we mirror
  // the resulting set into the store as canonical truth. Single-
  // element changes already routed through onNodeClick / onEdgeClick,
  // so we ignore them to avoid re-entry.
  const onSelectionChange: OnSelectionChangeFunc = useCallback(
    ({ nodes: rfNodes, edges: rfEdges }) => {
      if (rfNodes.length + rfEdges.length <= 1) return;
      const refs: SelectionRef[] = [];
      for (const n of rfNodes) {
        if (n.type === "group") continue;
        refs.push({ kind: "node", id: n.id });
      }
      for (const e of rfEdges) {
        refs.push({ kind: "edge", id: e.id });
      }
      if (refs.length === 0) return;
      selectMany(refs);
    },
    [selectMany],
  );

  const applyPositions = useCallback(
    (positions: Record<string, { x: number; y: number }>) => {
      setNodes((prev) =>
        prev.map((n) => {
          const pos = positions[n.id];
          return pos ? { ...n, position: { x: pos.x, y: pos.y } } : n;
        }),
      );
    },
    [setNodes],
  );

  if (!ontology) return <CanvasEmptyState />;
  if (arr(ontology.node_types).length === 0) return <CanvasZeroNodesState />;

  return (
    <div className="relative h-full w-full">
      <CanvasSkeleton visible={!layoutReady} />
      <CanvasFlow
        nodes={nodes}
        edges={edges}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        onNodeClick={onNodeClick}
        onEdgeClick={onEdgeClick}
        onNodeDragStop={onNodeDragStop}
        onNodeDoubleClick={onNodeDoubleClick}
        onNodeContextMenu={handleNodeContextMenu}
        onEdgeContextMenu={handleEdgeContextMenu}
        onPaneClick={onPaneClick}
        onSelectionChange={onSelectionChange}
      />

      <CanvasToolbar
        nodes={nodes}
        ontologyName={ontology.name}
        topologySignature={topologySignature}
        isExportOpen={isExportOpen}
        setIsExportOpen={setIsExportOpen}
        onExportSchema={handleExport}
        onApplyPositions={applyPositions}
        layout={layout}
        onLayoutChange={setLayout}
      />
      <NeighborhoodToolbar />
      <DiffOverlayBar />
      <VersionDiffBar />
      <CommandBar />

      {contextMenu.state && (
        <ContextMenu
          state={{
            type: contextMenu.state.target.type,
            id: contextMenu.state.target.id,
            x: contextMenu.state.x,
            y: contextMenu.state.y,
          }}
          items={
            contextMenu.state.target.type === "node"
              ? nodeContextMenuItems
              : edgeContextMenuItems
          }
          onClose={contextMenu.close}
        />
      )}

      <CanvasCommandSource actions={canvasCommandActions} />
    </div>
  );
}

export function OntologyCanvas({ gaps }: { gaps: QualityGap[] }) {
  return (
    <ReactFlowProvider>
      <CanvasInner gaps={gaps} />
    </ReactFlowProvider>
  );
}
