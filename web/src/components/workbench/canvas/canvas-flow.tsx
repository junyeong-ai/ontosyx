"use client";

import {
  type Node,
  type Edge,
  type NodeMouseHandler,
  type EdgeMouseHandler,
  type OnNodesChange,
  type OnEdgesChange,
} from "@xyflow/react";

import { SchemaNode } from "./schema-node";
import { SchemaEdge } from "./schema-edge";
import { GroupNode } from "./node-group";
import { GraphCanvas } from "./graph-canvas";
import { EdgeKindMarkers } from "./edge-kind-markers";

// ---------------------------------------------------------------------------
// CanvasFlow — ontology-specific adapter over `GraphCanvas`
// ---------------------------------------------------------------------------
//
// The shared XyFlow scaffolding (Background, Controls, MiniMap, handler
// surface, interaction defaults) lives in `GraphCanvas`. `CanvasFlow`
// supplies the ontology-specific pieces: the schema/group node renderer
// registry, the `schema` edge renderer, and the quality-layer-aware
// minimap color.

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
 * ReactFlow canvas surface for ontology editing. Delegates the shell to
 * `GraphCanvas`; specializes with schema node/edge renderers and the
 * quality-layer minimap coloring.
 */
export function CanvasFlow(props: CanvasFlowProps) {
  return (
    <>
      <EdgeKindMarkers />
      <GraphCanvas
        nodes={props.nodes}
        edges={props.edges}
        nodeTypes={nodeTypes}
        edgeTypes={edgeTypes}
        onNodesChange={props.onNodesChange}
        onEdgesChange={props.onEdgesChange}
        onNodeClick={props.onNodeClick}
        onEdgeClick={props.onEdgeClick}
        onNodeDragStop={props.onNodeDragStop}
        onNodeDoubleClick={props.onNodeDoubleClick}
        onNodeContextMenu={props.onNodeContextMenu}
        onEdgeContextMenu={props.onEdgeContextMenu}
        onPaneClick={props.onPaneClick}
        className="bg-zinc-50 dark:bg-zinc-950"
        minimap={{ nodeColor: ontologyMiniMapColor }}
      />
    </>
  );
}

function ontologyMiniMapColor(node: Node): string {
  const data = node.data as Record<string, unknown> | undefined;
  const layer = data?.layer as string | undefined;
  if (layer === "problematic") return "#ef4444";
  if (layer === "suggested") return "#0ea5e9";
  if (layer === "asserted") return "#10b981";
  return "#a1a1aa";
}
