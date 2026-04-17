"use client";

import {
  ReactFlow,
  Background,
  Controls,
  MiniMap,
  type Node,
  type Edge,
  type NodeMouseHandler,
  type EdgeMouseHandler,
  type OnNodesChange,
  type OnEdgesChange,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";

import { SchemaNode } from "./schema-node";
import { SchemaEdge } from "./schema-edge";
import { GroupNode } from "./node-group";

const nodeTypes = { schema: SchemaNode, group: GroupNode };
const edgeTypes = { schema: SchemaEdge };

interface CanvasFlowProps {
  nodes: Node[];
  edges: Edge[];
  onNodesChange: OnNodesChange;
  onEdgesChange: OnEdgesChange;
  onNodeClick: NodeMouseHandler;
  onEdgeClick: EdgeMouseHandler;
  onNodeDragStop: NodeMouseHandler;
  onNodeDoubleClick: NodeMouseHandler;
  onNodeContextMenu: NodeMouseHandler;
  onEdgeContextMenu: EdgeMouseHandler;
  onPaneClick: () => void;
}

/**
 * ReactFlow canvas surface with Background, Controls, and MiniMap decoration.
 *
 * Holds the stable node/edge type registrations and the minimap coloring
 * function; everything else is supplied by the parent canvas.
 */
export function CanvasFlow(props: CanvasFlowProps) {
  return (
    <ReactFlow
      nodes={props.nodes}
      edges={props.edges}
      onNodesChange={props.onNodesChange}
      onEdgesChange={props.onEdgesChange}
      onNodeClick={props.onNodeClick}
      onEdgeClick={props.onEdgeClick}
      onNodeDragStop={props.onNodeDragStop}
      onNodeDoubleClick={props.onNodeDoubleClick}
      onNodeContextMenu={props.onNodeContextMenu}
      onEdgeContextMenu={props.onEdgeContextMenu}
      onPaneClick={props.onPaneClick}
      nodeTypes={nodeTypes}
      edgeTypes={edgeTypes}
      fitView
      proOptions={{ hideAttribution: true }}
      minZoom={0.1}
      maxZoom={2}
      nodesDraggable={true}
      nodesConnectable={false}
      elementsSelectable={true}
      selectNodesOnDrag={false}
      zoomOnDoubleClick={false}
      onlyRenderVisibleElements={true}
      className="bg-zinc-50 dark:bg-zinc-950"
    >
      <Background gap={20} size={1} color="#e4e4e7" />
      <Controls
        showInteractive={false}
        className="!rounded-lg !border-zinc-200 !bg-white !shadow-sm dark:!border-zinc-700 dark:!bg-zinc-900"
      />
      <MiniMap
        pannable
        zoomable
        nodeStrokeWidth={3}
        nodeColor={(node) => {
          const data = node.data as Record<string, unknown> | undefined;
          const layer = data?.layer as string | undefined;
          if (layer === "problematic") return "#ef4444";
          if (layer === "suggested") return "#0ea5e9";
          if (layer === "asserted") return "#10b981";
          return "#a1a1aa";
        }}
        maskColor="rgba(0,0,0,0.08)"
        className="!rounded-lg !border-zinc-200 !bg-white dark:!border-zinc-700 dark:!bg-zinc-900"
      />
    </ReactFlow>
  );
}
