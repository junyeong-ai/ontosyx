"use client";

import { useCallback, useMemo, useState } from "react";
import {
  useNodesState,
  useEdgesState,
  ReactFlowProvider,
  type Node,
  type Edge,
  type NodeMouseHandler,
  type EdgeMouseHandler,
} from "@xyflow/react";

import { CommandBar, DiffOverlayBar, VersionDiffBar } from "./command-bar";
import { ContextMenu } from "./context-menu";
import { CommandPalette } from "./command-palette";
import { NeighborhoodToolbar } from "./neighborhood-toolbar";
import { exportCanvasImage } from "./canvas-helpers";
import { useCanvasLayout } from "./use-canvas-layout";
import { CanvasSkeleton } from "./canvas-skeleton";
import { CanvasEmptyState } from "./canvas-empty-state";
import { CanvasToolbar } from "./canvas-toolbar";
import { CanvasFlow } from "./canvas-flow";
import { useCanvasState } from "./use-canvas-state";
import { useCanvasCommands } from "@/lib/store/canvas/commands";
import { useCanvasKeyboard } from "@/lib/store/canvas/keyboard";
import { useCanvasContextMenu } from "@/lib/store/canvas/context-menu";
import { useCanvasSelection } from "@/lib/store/canvas/selection";
import { useCanvasViewport } from "@/lib/store/canvas/viewport";
import type { QualityGap } from "@/types/api";

function CanvasInner({ gaps }: { gaps: QualityGap[] }) {
  const { ontology, select, clearSelection, setHighlightedBindings, setNeighborhoodFocus } = useCanvasState();

  const [nodes, setNodes, onNodesChange] = useNodesState<Node>([]);
  const [edges, setEdges, onEdgesChange] = useEdgesState<Edge>([]);

  const [isExportOpen, setIsExportOpen] = useState(false);
  const [isPaletteOpen, setIsPaletteOpen] = useState(false);

  const { flowElements, topologySignature } = useCanvasViewport(gaps);

  useCanvasSelection({ ontology, setNodes, setEdges });

  const { onNodeDragStop, runAutoLayout, layoutReady } = useCanvasLayout(
    flowElements,
    topologySignature,
    setNodes,
    setEdges,
  );

  const { handleSave, deleteSelected, selectAllNodes, handleExport, deselectAll } = useCanvasCommands({
    setIsPaletteOpen,
    setIsExportOpen,
  });

  const { paletteCommands: getPaletteCommands } = useCanvasKeyboard({
    handleSave,
    deleteSelected,
    runAutoLayout: () => runAutoLayout(nodes, edges),
    selectAllNodes,
    deselectAll,
    exportPng: () => exportCanvasImage(nodes, "png", ontology?.name ?? "ontology"),
    exportSvg: () => exportCanvasImage(nodes, "svg", ontology?.name ?? "ontology"),
    setIsPaletteOpen,
  });
  const memoizedPaletteCommands = useMemo(() => getPaletteCommands(), [getPaletteCommands]);

  const {
    contextMenu,
    closeContextMenu,
    handleNodeContextMenu,
    handleEdgeContextMenu,
    nodeContextMenuItems,
    edgeContextMenuItems,
  } = useCanvasContextMenu(ontology);

  const onNodeClick: NodeMouseHandler = useCallback(
    (_event, node) => {
      closeContextMenu();
      if (node.type === "group") return;
      select({ type: "node", nodeId: node.id });
    },
    [select, closeContextMenu],
  );

  const onNodeDoubleClick: NodeMouseHandler = useCallback(
    (_event, node) => {
      if (node.type === "group") return;
      setNeighborhoodFocus({ nodeId: node.id, depth: 1 });
    },
    [setNeighborhoodFocus],
  );

  const onEdgeClick: EdgeMouseHandler = useCallback(
    (_event, edge) => {
      closeContextMenu();
      select({ type: "edge", edgeId: edge.id });
    },
    [select, closeContextMenu],
  );

  const onPaneClick = useCallback(() => {
    closeContextMenu();
    clearSelection();
    setHighlightedBindings(null);
  }, [clearSelection, setHighlightedBindings, closeContextMenu]);

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
      />

      <CanvasToolbar
        nodes={nodes}
        ontologyName={ontology.name}
        topologySignature={topologySignature}
        isExportOpen={isExportOpen}
        setIsExportOpen={setIsExportOpen}
        onExportSchema={handleExport}
        onApplyPositions={applyPositions}
      />
      <NeighborhoodToolbar />
      <DiffOverlayBar />
      <VersionDiffBar />
      <CommandBar />

      {contextMenu && (
        <ContextMenu
          state={contextMenu}
          items={contextMenu.type === "node" ? nodeContextMenuItems : edgeContextMenuItems}
          onClose={closeContextMenu}
        />
      )}

      {isPaletteOpen && (
        <CommandPalette
          open={isPaletteOpen}
          onClose={() => setIsPaletteOpen(false)}
          commands={memoizedPaletteCommands}
        />
      )}
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
