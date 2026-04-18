"use client";

import { memo, useCallback, useEffect, useMemo, useState } from "react";
import {
  ReactFlowProvider,
  BaseEdge,
  EdgeLabelRenderer,
  Handle,
  Position,
  MarkerType,
  getBezierPath,
  type Node,
  type Edge,
  type NodeProps,
  type EdgeProps,
  type NodeMouseHandler,
} from "@xyflow/react";

import { GraphCanvas } from "@/components/workbench/canvas/graph-canvas";
import { useIsDarkMode } from "@/lib/use-dark-mode";
import { useTypeFilter } from "@/lib/use-type-filter";
import type { ExpandNeighbor, GraphOverview } from "@/lib/api/queries";

import { resolveDisplayName, resolveNodeColor } from "./graph-utils";

// ---------------------------------------------------------------------------
// ExploreCanvas — XyFlow-based successor to the NVL `ExploreGraphView`
// ---------------------------------------------------------------------------
//
// Two render modes, same component:
//
//   1. Neighborhood mode (`focusedNode` set) — the focused node plus its
//      1-hop neighbors. Click any neighbor to refocus via `onNodeClick`.
//   2. Schema mode (`focusedNode` null, `schemaOverview` populated) —
//      every label in the graph sized by node count, edges sized by
//      relationship count. Click a label to pivot into neighborhood
//      mode for a representative node.
//
// Layout: ELK's `stress` algorithm computed on the main thread. Explore
// graphs are small (≤ ~50 nodes) and the layout runs only when the data
// reference changes, so the worker path isn't worth the ceremony here.
// The result is post-processed by centering the focused node at (0, 0)
// so neighborhood views feel stable across pivots.
//
// Replaces `graph-view.tsx` (NVL-based). Props match that file's
// `ExploreGraphViewProps` so the explore page swaps with no call-site
// changes.

// ---------------------------------------------------------------------------
// Public types — mirror `graph-view.tsx` so callers swap without churn
// ---------------------------------------------------------------------------

export interface FocusedNode {
  elementId: string;
  labels: string[];
  props: Record<string, unknown>;
}

export interface ExploreCanvasProps {
  focusedNode: FocusedNode | null;
  neighbors: ExpandNeighbor[];
  schemaOverview: GraphOverview | null;
  onNodeClick: (nodeId: string) => void;
}

// ---------------------------------------------------------------------------
// Node/edge data + renderers
// ---------------------------------------------------------------------------

interface ExploreNodeData extends Record<string, unknown> {
  caption: string;
  subtitle?: string;
  color: string;
  /** Visual radius in pixels (scaled by count in schema mode). */
  radius: number;
  focused: boolean;
  label: string;
}

function ExploreNodeRenderer({ data }: NodeProps & { data: ExploreNodeData }) {
  const diameter = data.radius * 2;
  return (
    <div
      className={`group relative flex flex-col items-center ${
        data.focused ? "z-10" : ""
      }`}
      style={{ width: diameter }}
    >
      <Handle type="target" position={Position.Top} className="!h-1 !w-1 !opacity-0" />
      <Handle type="source" position={Position.Bottom} className="!h-1 !w-1 !opacity-0" />
      <div
        className={`flex items-center justify-center rounded-full border-2 text-white transition-shadow ${
          data.focused
            ? "border-emerald-300 shadow-[0_0_0_3px_rgba(16,185,129,0.25)]"
            : "border-white/60 shadow-sm dark:border-zinc-800"
        }`}
        style={{
          width: diameter,
          height: diameter,
          backgroundColor: data.color,
          fontSize: Math.max(9, Math.min(13, data.radius * 0.45)),
          fontWeight: 600,
        }}
      >
        {data.caption.split("\n")[0]}
      </div>
      <div className="pointer-events-none mt-1 max-w-[140px] truncate rounded bg-white/80 px-1 text-[10px] text-zinc-600 shadow-sm backdrop-blur dark:bg-zinc-900/80 dark:text-zinc-300">
        {data.subtitle ?? data.caption}
      </div>
    </div>
  );
}

interface ExploreEdgeData extends Record<string, unknown> {
  caption: string;
  width: number;
}

function ExploreEdgeRenderer(props: EdgeProps) {
  const { id, sourceX, sourceY, targetX, targetY, sourcePosition, targetPosition, markerEnd } = props;
  const data = props.data as ExploreEdgeData | undefined;
  const [path, labelX, labelY] = getBezierPath({
    sourceX,
    sourceY,
    sourcePosition,
    targetX,
    targetY,
    targetPosition,
    curvature: 0.25,
  });
  return (
    <>
      <BaseEdge
        id={id}
        path={path}
        markerEnd={markerEnd}
        style={{ stroke: "#a1a1aa", strokeWidth: data?.width ?? 1 }}
      />
      {data?.caption && (
        <EdgeLabelRenderer>
          <div
            style={{ transform: `translate(-50%, -50%) translate(${labelX}px, ${labelY}px)` }}
            className="pointer-events-none rounded-full border border-zinc-200 bg-white px-1.5 py-0.5 text-[9px] text-zinc-500 shadow-sm dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-400"
          >
            {data.caption}
          </div>
        </EdgeLabelRenderer>
      )}
    </>
  );
}

const nodeTypesRegistry = { explore: memo(ExploreNodeRenderer) };
const edgeTypesRegistry = { explore: memo(ExploreEdgeRenderer) };

// ---------------------------------------------------------------------------
// Layout — ELK `stress` algorithm on the main thread
// ---------------------------------------------------------------------------

interface LayoutInput {
  id: string;
  width: number;
  height: number;
}
interface LayoutEdge {
  id: string;
  source: string;
  target: string;
}

async function computeExploreLayout(
  nodes: LayoutInput[],
  edges: LayoutEdge[],
): Promise<Record<string, { x: number; y: number }>> {
  if (nodes.length === 0) return {};
  const ELK = (await import("elkjs/lib/elk.bundled.js")).default;
  const elk = new ELK();
  const graph = {
    id: "explore-root",
    layoutOptions: {
      // `stress` finds a placement that minimises edge-length distortion —
      // the closest ELK comes to a force-directed aesthetic for general
      // (non-hierarchical) graphs without pulling in a separate d3-force
      // dep. `spacing.nodeNode` is generous so labels have breathing room.
      "elk.algorithm": "stress",
      "elk.spacing.nodeNode": "80",
      "elk.stress.epsilon": "0.0001",
      "elk.stress.iterationLimit": "400",
    },
    children: nodes.map((n) => ({ id: n.id, width: n.width, height: n.height })),
    edges: edges.map((e) => ({ id: e.id, sources: [e.source], targets: [e.target] })),
  };
  const laid = await elk.layout(graph);
  const out: Record<string, { x: number; y: number }> = {};
  for (const child of laid.children ?? []) {
    out[child.id] = { x: child.x ?? 0, y: child.y ?? 0 };
  }
  return out;
}

// ---------------------------------------------------------------------------
// Data builders — mirror the NVL version's shapes, emit XyFlow nodes/edges
// ---------------------------------------------------------------------------

interface BuiltGraph {
  nodes: Node[];
  edges: Edge[];
  legendLabels: string[];
  /** Id of the focused node (if any) — used to re-center post-layout. */
  focusedId: string | null;
}

function buildNeighborhoodGraph(focusedNode: FocusedNode, neighbors: ExpandNeighbor[]): BuiltGraph {
  const focusedLabel = focusedNode.labels[0] || "Node";
  const nodeMap = new Map<string, Node>();
  const labelSet = new Set<string>();

  labelSet.add(focusedLabel);
  const focusedRadius = 32;
  nodeMap.set(focusedNode.elementId, {
    id: focusedNode.elementId,
    type: "explore",
    position: { x: 0, y: 0 },
    data: {
      caption: focusedLabel,
      subtitle: resolveDisplayName(focusedNode.props, focusedLabel),
      color: resolveNodeColor(focusedLabel, true),
      radius: focusedRadius,
      focused: true,
      label: focusedLabel,
    } satisfies ExploreNodeData,
  });

  for (const n of neighbors) {
    if (nodeMap.has(n.element_id)) continue;
    const nLabel = n.labels[0] || "Node";
    labelSet.add(nLabel);
    nodeMap.set(n.element_id, {
      id: n.element_id,
      type: "explore",
      position: { x: 0, y: 0 },
      data: {
        caption: nLabel,
        subtitle: resolveDisplayName(n.props, nLabel),
        color: resolveNodeColor(nLabel, false),
        radius: 20,
        focused: false,
        label: nLabel,
      } satisfies ExploreNodeData,
    });
  }

  const edges: Edge[] = neighbors.map((n, i) => ({
    id: `rel-${i}`,
    source: n.direction === "outgoing" ? focusedNode.elementId : n.element_id,
    target: n.direction === "outgoing" ? n.element_id : focusedNode.elementId,
    type: "explore",
    markerEnd: { type: MarkerType.ArrowClosed, width: 12, height: 12 },
    data: {
      caption: n.relationship_type || "",
      width: 1,
    } satisfies ExploreEdgeData,
  }));

  return {
    nodes: Array.from(nodeMap.values()),
    edges,
    legendLabels: Array.from(labelSet),
    focusedId: focusedNode.elementId,
  };
}

function buildSchemaGraph(overview: GraphOverview): BuiltGraph {
  const labelSet = new Set<string>();
  const maxCount = Math.max(...overview.labels.map((l) => l.count), 1);

  const nodes: Node[] = overview.labels.map((l) => {
    labelSet.add(l.label);
    const sizeScale = Math.log10(l.count + 1) / Math.log10(maxCount + 1);
    const radius = 18 + sizeScale * 24;
    return {
      id: `schema:${l.label}`,
      type: "explore",
      position: { x: 0, y: 0 },
      data: {
        caption: l.label,
        subtitle: `${l.label} · ${l.count.toLocaleString()}`,
        color: resolveNodeColor(l.label, false),
        radius,
        focused: false,
        label: l.label,
      } satisfies ExploreNodeData,
    };
  });

  const nodeIds = new Set(nodes.map((n) => n.id));
  const edges: Edge[] = overview.relationships
    .map((r, i) => ({
      id: `schema-rel-${i}`,
      source: `schema:${r.from_label}`,
      target: `schema:${r.to_label}`,
      type: "explore",
      markerEnd: { type: MarkerType.ArrowClosed, width: 12, height: 12 },
      data: {
        caption: r.rel_type,
        width: Math.max(1, Math.min(3, Math.log10(r.count + 1))),
      } satisfies ExploreEdgeData,
    }))
    .filter((e) => nodeIds.has(e.source) && nodeIds.has(e.target));

  return { nodes, edges, legendLabels: Array.from(labelSet), focusedId: null };
}

// ---------------------------------------------------------------------------
// ExploreCanvas
// ---------------------------------------------------------------------------

function ExploreCanvasInner({ focusedNode, neighbors, schemaOverview, onNodeClick }: ExploreCanvasProps) {
  const isDark = useIsDarkMode();
  const isSchemaMode = !focusedNode && !!schemaOverview && schemaOverview.labels.length > 0;

  const built = useMemo<BuiltGraph>(() => {
    if (focusedNode) return buildNeighborhoodGraph(focusedNode, neighbors);
    if (schemaOverview && schemaOverview.labels.length > 0) return buildSchemaGraph(schemaOverview);
    return { nodes: [], edges: [], legendLabels: [], focusedId: null };
  }, [focusedNode, neighbors, schemaOverview]);

  // Filter by node type (legend chips) — mirrors the widget graph's pattern.
  const typeFilter = useTypeFilter<Node, Edge>({
    allTypes: built.legendLabels,
    getNodeType: (n) => String((n.data as ExploreNodeData).label),
    getEdgeSource: (e) => e.source,
    getEdgeTarget: (e) => e.target,
  });

  // Re-layout every time the underlying graph reference changes. ELK is
  // fast enough on the main thread for the sizes explore deals with;
  // a brief loading indicator bridges the gap on slower machines.
  const [layoutNodes, setLayoutNodes] = useState<Node[]>([]);
  const [laying, setLaying] = useState(false);

  useEffect(() => {
    let cancelled = false;
    if (built.nodes.length === 0) {
      setLayoutNodes([]);
      return;
    }
    setLaying(true);
    const layoutInputs: LayoutInput[] = built.nodes.map((n) => {
      const r = (n.data as ExploreNodeData).radius;
      const diameter = r * 2;
      return { id: n.id, width: diameter, height: diameter + 20 /* label */ };
    });
    const layoutEdges: LayoutEdge[] = built.edges.map((e) => ({
      id: e.id,
      source: e.source,
      target: e.target,
    }));
    computeExploreLayout(layoutInputs, layoutEdges)
      .then((positions) => {
        if (cancelled) return;
        // Center on the focused node, if any, so pivots feel stable.
        const center = built.focusedId ? positions[built.focusedId] : undefined;
        const ox = center?.x ?? 0;
        const oy = center?.y ?? 0;
        setLayoutNodes(
          built.nodes.map((n) => ({
            ...n,
            position: positions[n.id]
              ? { x: positions[n.id].x - ox, y: positions[n.id].y - oy }
              : n.position,
          })),
        );
      })
      .catch((err) => {
        // A layout failure falls back to the incoming (0,0) placements;
        // degraded but visible rather than crashing the page.
        if (!cancelled) {
          console.warn("[explore-canvas] ELK layout failed, falling back to zero positions:", err);
          setLayoutNodes(built.nodes);
        }
      })
      .finally(() => {
        if (!cancelled) setLaying(false);
      });
    return () => {
      cancelled = true;
    };
  }, [built]);

  const visibleNodes = useMemo(() => typeFilter.filterNodes(layoutNodes), [typeFilter, layoutNodes]);
  const visibleIds = useMemo(() => new Set(visibleNodes.map((n) => n.id)), [visibleNodes]);
  const visibleEdges = useMemo(
    () => typeFilter.filterEdges(built.edges, visibleIds),
    [typeFilter, built.edges, visibleIds],
  );

  const handleNodeClick: NodeMouseHandler = useCallback(
    (_e, node) => {
      const id = node.id;
      if (isSchemaMode) {
        onNodeClick(id);
        return;
      }
      // In neighborhood mode clicking the focused node itself is a no-op —
      // re-focusing on yourself would just re-run the same query.
      if (built.focusedId && id === built.focusedId) return;
      onNodeClick(id);
    },
    [isSchemaMode, onNodeClick, built.focusedId],
  );

  const bg = isDark ? "#09090b" : "#fafafa";

  if (built.nodes.length === 0) {
    return (
      <div
        className="flex h-full items-center justify-center text-sm text-muted-foreground"
        style={{ backgroundColor: bg }}
      >
        {focusedNode ? "No neighbors found" : "Loading graph schema..."}
      </div>
    );
  }

  return (
    <div className="relative h-full w-full" style={{ backgroundColor: bg }}>
      <GraphCanvas
        nodes={visibleNodes}
        edges={visibleEdges}
        nodeTypes={nodeTypesRegistry}
        edgeTypes={edgeTypesRegistry}
        onNodeClick={handleNodeClick}
        minZoom={0.2}
        maxZoom={3}
        nodesDraggable={false}
        elementsSelectable
      />
      {laying && (
        <div className="pointer-events-none absolute inset-0 flex items-center justify-center text-sm text-muted-foreground">
          Laying out graph…
        </div>
      )}
      {isSchemaMode && !laying && (
        <div className="pointer-events-none absolute right-3 top-3 rounded bg-white/80 px-2.5 py-1 text-[10px] text-zinc-500 shadow-sm dark:bg-zinc-900/80 dark:text-zinc-400">
          Data Model — click a node to explore
        </div>
      )}
      {built.legendLabels.length > 0 && (
        <div className="absolute left-3 top-3 z-10 flex max-w-[60%] flex-wrap gap-1">
          {built.legendLabels.map((label) => {
            const hidden = typeFilter.hiddenTypes.has(label);
            const focused = !isSchemaMode && label === (focusedNode?.labels[0] || "");
            return (
              <button
                key={label}
                type="button"
                onClick={() => typeFilter.toggle(label)}
                aria-pressed={!hidden}
                aria-label={`${hidden ? "Show" : "Hide"} ${label} nodes`}
                className={`flex cursor-pointer items-center gap-1 rounded-full bg-white/80 px-2 py-0.5 text-[10px] shadow-sm transition-opacity dark:bg-zinc-900/80 ${
                  hidden
                    ? "text-zinc-400 line-through opacity-60 dark:text-zinc-500"
                    : "text-zinc-500 dark:text-zinc-400"
                }`}
              >
                <span
                  className="h-2 w-2 rounded-full"
                  style={{ backgroundColor: resolveNodeColor(label, focused) }}
                />
                {label}
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}

/**
 * Public entry point — wrapped in `ReactFlowProvider` so the canvas can
 * call into `useReactFlow` (future `fitView` / `zoomToFit` hooks).
 */
export function ExploreCanvas(props: ExploreCanvasProps) {
  return (
    <ReactFlowProvider>
      <ExploreCanvasInner {...props} />
    </ReactFlowProvider>
  );
}
