"use client";

import { type ReactNode } from "react";
import {
  ReactFlow,
  Background,
  Controls,
  MiniMap,
  type Node,
  type Edge,
  type NodeTypes,
  type EdgeTypes,
  type NodeMouseHandler,
  type EdgeMouseHandler,
  type OnNodesChange,
  type OnEdgesChange,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";

// ---------------------------------------------------------------------------
// GraphCanvas — shared XyFlow shell for every graph surface in the workbench
// ---------------------------------------------------------------------------
//
// The ontology canvas, the query canvas, and the upcoming explore canvas
// each draw a different flavour of graph — different node/edge renderers,
// different zoom policy, different minimap coloring — but they all want
// the same scaffolding:
//
//   * ReactFlow instance with Background + Controls.
//   * An optional MiniMap with caller-supplied node-color mapping.
//   * Standard interaction handler surface (click / context-menu / drag /
//     pane-click / change) exposed as props so callers wire only what
//     they need.
//   * A `children` slot for overlays (drop zones, toolbars, diff bars)
//     that ReactFlow renders inside the viewport coordinate space.
//
// `GraphCanvas` is deliberately UN-provided — it does NOT wrap a
// `ReactFlowProvider`. Callers handle that themselves because most of
// them also need `useReactFlow` for `screenToFlowPosition` or
// `fitView`, and the provider must live at or above that hook's call
// site. One provider per canvas root is the right layering.

export interface GraphCanvasProps {
  nodes: Node[];
  edges: Edge[];
  nodeTypes: NodeTypes;
  edgeTypes?: EdgeTypes;

  // --- Interaction handlers --------------------------------------------
  onNodesChange?: OnNodesChange;
  onEdgesChange?: OnEdgesChange;
  onNodeClick?: NodeMouseHandler;
  onEdgeClick?: EdgeMouseHandler;
  onNodeDragStop?: NodeMouseHandler;
  onNodeDoubleClick?: NodeMouseHandler;
  onNodeContextMenu?: NodeMouseHandler;
  onEdgeContextMenu?: EdgeMouseHandler;
  onPaneClick?: () => void;

  // --- Interaction policy ----------------------------------------------
  //
  // Defaults track the OntologyCanvas sensibilities: drag on, connect
  // off, double-click zoom off. Any canvas with different needs can
  // opt out explicitly.
  nodesDraggable?: boolean;
  nodesConnectable?: boolean;
  elementsSelectable?: boolean;
  selectNodesOnDrag?: boolean;
  zoomOnDoubleClick?: boolean;
  onlyRenderVisibleElements?: boolean;
  fitView?: boolean;
  minZoom?: number;
  maxZoom?: number;

  className?: string;

  /**
   * MiniMap configuration. `false` disables the minimap; an object
   * configures its `nodeColor`. Omit entirely to also disable —
   * minimap is opt-in.
   */
  minimap?: false | { nodeColor: (node: Node) => string };

  /** Overlays rendered inside the ReactFlow viewport. */
  children?: ReactNode;
}

export function GraphCanvas(props: GraphCanvasProps) {
  const {
    nodes,
    edges,
    nodeTypes,
    edgeTypes,
    onNodesChange,
    onEdgesChange,
    onNodeClick,
    onEdgeClick,
    onNodeDragStop,
    onNodeDoubleClick,
    onNodeContextMenu,
    onEdgeContextMenu,
    onPaneClick,
    nodesDraggable = true,
    nodesConnectable = false,
    elementsSelectable = true,
    selectNodesOnDrag = false,
    zoomOnDoubleClick = false,
    onlyRenderVisibleElements = true,
    fitView = true,
    minZoom = 0.1,
    maxZoom = 2,
    className,
    minimap,
    children,
  } = props;

  return (
    <ReactFlow
      nodes={nodes}
      edges={edges}
      onNodesChange={onNodesChange}
      onEdgesChange={onEdgesChange}
      onNodeClick={onNodeClick}
      onEdgeClick={onEdgeClick}
      onNodeDragStop={onNodeDragStop}
      onNodeDoubleClick={onNodeDoubleClick}
      onNodeContextMenu={onNodeContextMenu}
      onEdgeContextMenu={onEdgeContextMenu}
      onPaneClick={onPaneClick}
      nodeTypes={nodeTypes}
      edgeTypes={edgeTypes}
      fitView={fitView}
      proOptions={{ hideAttribution: true }}
      minZoom={minZoom}
      maxZoom={maxZoom}
      nodesDraggable={nodesDraggable}
      nodesConnectable={nodesConnectable}
      elementsSelectable={elementsSelectable}
      selectNodesOnDrag={selectNodesOnDrag}
      zoomOnDoubleClick={zoomOnDoubleClick}
      onlyRenderVisibleElements={onlyRenderVisibleElements}
      className={className}
    >
      <Background gap={20} size={1} color="#e4e4e7" />
      <Controls
        showInteractive={false}
        className="!rounded-lg !border-divider !bg-surface-base !shadow-sm dark:!border-divider"
      />
      {minimap && (
        <MiniMap
          pannable
          zoomable
          nodeStrokeWidth={3}
          nodeColor={minimap.nodeColor}
          maskColor="rgba(0,0,0,0.08)"
          className="!rounded-lg !border-divider !bg-surface-base dark:!border-divider"
        />
      )}
      {children}
    </ReactFlow>
  );
}
