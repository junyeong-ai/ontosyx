"use client";

import {
  memo,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import dynamic from "next/dynamic";
import type { ForceGraphMethods } from "react-force-graph-2d";
import type {
  QueryResult,
  WidgetSpec,
  GraphLayout,
} from "@/types/api";
import { cn } from "@/lib/cn";
import { useIsDarkMode } from "@/lib/use-dark-mode";
import { useContainerWidth } from "@/lib/use-container-width";
import { formatValue } from "./chart-utils";
import type { GraphNodeData, FGNode, FGLink } from "./graph/graph-types";
import { DEFAULT_MAX_NODES, DARK_BG, LIGHT_BG } from "./graph/graph-constants";
import { extractGraphData } from "./graph/graph-data";
import { buildTooltipHtml, layoutToDagMode } from "./graph/graph-utils";
import { NodeDetailPanel } from "./graph/graph-detail-panel";
import { Legend } from "./graph/graph-legend";

// ---------------------------------------------------------------------------
// react-force-graph-2d uses Canvas + DOM APIs that are unavailable during SSR.
// Dynamic import with ssr:false ensures it only loads on the client.
// ---------------------------------------------------------------------------
const ForceGraph2D = dynamic(() => import("react-force-graph-2d"), {
  ssr: false,
});

// ---------------------------------------------------------------------------
// GraphWidget — main component
// ---------------------------------------------------------------------------

interface GraphWidgetProps {
  spec: WidgetSpec;
  data: QueryResult;
}

export const GraphWidget = memo(function GraphWidget({
  spec,
  data,
}: GraphWidgetProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  // ForceGraphMethods with default generics — our extra fields are accessible
  // through the [others: string]: any index signature on NodeObject/LinkObject.
  const graphRef = useRef<ForceGraphMethods>(undefined);
  const containerWidth = useContainerWidth(containerRef);
  const isDark = useIsDarkMode();

  const [selectedNode, setSelectedNode] = useState<GraphNodeData | null>(null);
  const [hoveredNodeId, setHoveredNodeId] = useState<string | null>(null);

  const nodeConfig = spec.node_config;
  const edgeConfig = spec.edge_config;
  const layout = (spec.layout ?? "force") as GraphLayout;
  const maxNodes = spec.max_nodes ?? DEFAULT_MAX_NODES;

  const extracted = useMemo(
    () => extractGraphData(data, nodeConfig, edgeConfig, maxNodes),
    [data, nodeConfig, edgeConfig, maxNodes],
  );

  // Build type-color index for legend
  const typeColorIndex = useMemo(() => {
    const idx = new Map<string, string>();
    for (const node of extracted.nodes) {
      const key = node.type ?? "default";
      if (!idx.has(key)) {
        idx.set(key, node.__color);
      }
    }
    return idx;
  }, [extracted.nodes]);

  // react-force-graph expects { nodes, links } — our custom fields survive
  // because NodeObject/LinkObject have [others: string]: any.
  const graphData = useMemo(
    () => ({ nodes: extracted.nodes, links: extracted.links }),
    [extracted],
  );

  const dagMode = layoutToDagMode(layout);
  const isDirected = edgeConfig?.directed ?? true;
  const isTruncated = extracted.totalNodes > maxNodes;

  // Zoom to fit after initial render
  useEffect(() => {
    const timer = setTimeout(() => {
      graphRef.current?.zoomToFit(400, 40);
    }, 500);
    return () => clearTimeout(timer);
  }, [extracted]);

  const graphHeight = Math.min(400, Math.max(280, containerWidth * 0.6));

  // --- Callbacks ---
  // All callbacks receive the base NodeObject/LinkObject at runtime.
  // Our custom fields (label, __color, etc.) are accessible via the index signature.

  const handleNodeClick = useCallback(
    (node: Record<string, unknown>) => {
      const gn = node as unknown as FGNode;
      setSelectedNode((prev) => (prev?.id === gn.id ? null : gn));
    },
    [],
  );

  const handleNodeHover = useCallback(
    (node: Record<string, unknown> | null) => {
      const gn = node as unknown as FGNode | null;
      setHoveredNodeId(gn?.id ?? null);
    },
    [],
  );

  const handleBackgroundClick = useCallback(() => {
    setSelectedNode(null);
  }, []);

  // Custom node canvas rendering: circle + label
  const paintNode = useCallback(
    (
      node: Record<string, unknown>,
      ctx: CanvasRenderingContext2D,
      globalScale: number,
    ) => {
      const gn = node as unknown as FGNode;
      const x = gn.x ?? 0;
      const y = gn.y ?? 0;
      const r = gn.__size;
      const isSelected = selectedNode?.id === gn.id;
      const isHovered = hoveredNodeId === gn.id;

      // Node circle
      ctx.beginPath();
      ctx.arc(x, y, r, 0, 2 * Math.PI);
      ctx.fillStyle = gn.__color;
      ctx.fill();

      // Selection/hover ring
      if (isSelected || isHovered) {
        ctx.strokeStyle = isSelected
          ? (isDark ? "#10b981" : "#059669")
          : (isDark ? "#a1a1aa" : "#71717a");
        ctx.lineWidth = isSelected ? 2 : 1.5;
        ctx.stroke();
      }

      // Label — adaptive sizing with canvas-space cap to prevent overlap
      const baseFontSize = Math.min(12 / globalScale, 6);
      const selectedFontSize = Math.min(12 / globalScale, 9);
      const fontSize = isSelected || isHovered ? selectedFontSize : baseFontSize;
      const screenPx = fontSize * globalScale;

      if (screenPx >= 4 || isSelected || isHovered) {
        const maxChars = globalScale >= 1.5 ? Infinity : 10;
        const displayLabel =
          gn.label.length > maxChars
            ? gn.label.slice(0, maxChars) + "\u2026"
            : gn.label;

        ctx.font = `${isSelected ? "600" : "400"} ${fontSize}px Inter, system-ui, sans-serif`;
        ctx.textAlign = "center";
        ctx.textBaseline = "top";
        ctx.fillStyle = isDark ? "#e4e4e7" : "#3f3f46";
        ctx.fillText(displayLabel, x, y + r + 2);
      }
    },
    [selectedNode, hoveredNodeId, isDark],
  );

  // Custom node pointer area
  const paintNodeArea = useCallback(
    (
      node: Record<string, unknown>,
      color: string,
      ctx: CanvasRenderingContext2D,
    ) => {
      const gn = node as unknown as FGNode;
      const x = gn.x ?? 0;
      const y = gn.y ?? 0;
      const r = gn.__size + 4; // slightly larger hit area
      ctx.beginPath();
      ctx.arc(x, y, r, 0, 2 * Math.PI);
      ctx.fillStyle = color;
      ctx.fill();
    },
    [],
  );

  // Link label accessor
  const linkLabel = useCallback(
    (link: Record<string, unknown>): string => {
      const gl = link as unknown as FGLink;
      if (!gl.label) return "";
      const props = Object.entries(gl.properties)
        .filter(([, v]) => v != null)
        .map(([k, v]) => `${k}: ${formatValue(v)}`)
        .join("\n");
      return props ? `${gl.label}\n${props}` : gl.label;
    },
    [],
  );

  // Link color — lighter in dark mode for visibility
  const linkColor = useCallback(
    () => (isDark ? "#a1a1aa" : "#71717a"),
    [isDark],
  );

  // Link directional arrow color
  const arrowColor = useCallback(
    () => (isDark ? "#a1a1aa" : "#71717a"),
    [isDark],
  );

  // Node tooltip
  const nodeLabel = useCallback(
    (node: Record<string, unknown>): string => {
      const gn = node as unknown as FGNode;
      return buildTooltipHtml(gn, nodeConfig?.tooltip_fields);
    },
    [nodeConfig?.tooltip_fields],
  );

  // Empty state
  if (!data.rows.length || extracted.nodes.length === 0) {
    return (
      <div className="flex h-48 items-center justify-center rounded-lg border border-dashed border-zinc-300 dark:border-zinc-700">
        <p className="text-xs text-zinc-400">No graph data to display</p>
      </div>
    );
  }

  return (
    <div className="space-y-1.5">
      {spec.title && (
        <h4 className="text-xs font-semibold text-zinc-600 dark:text-zinc-400">
          {spec.title}
        </h4>
      )}
      <div
        ref={containerRef}
        className={cn(
          "relative overflow-hidden rounded-lg border",
          "border-zinc-200 dark:border-zinc-700",
          "bg-white dark:bg-zinc-900",
        )}
        role="figure"
        aria-label={spec.title ? `Graph: ${spec.title}` : "Graph visualization"}
      >
        {containerWidth > 0 && (
          <ForceGraph2D
            ref={graphRef}
            graphData={graphData}
            width={containerWidth}
            height={graphHeight}
            backgroundColor={isDark ? DARK_BG : LIGHT_BG}
            // Node rendering
            nodeCanvasObject={paintNode}
            nodeCanvasObjectMode={() => "replace"}
            nodePointerAreaPaint={paintNodeArea}
            nodeLabel={nodeLabel}
            // Link styling
            linkColor={linkColor}
            linkWidth={1.5}
            linkLabel={linkLabel}
            linkDirectionalArrowLength={isDirected ? 5 : 0}
            linkDirectionalArrowRelPos={1}
            linkDirectionalArrowColor={arrowColor}
            linkCurvature={0.15}
            // Layout
            dagMode={dagMode}
            dagLevelDistance={50}
            d3VelocityDecay={0.3}
            cooldownTicks={100}
            // Interaction
            onNodeClick={handleNodeClick}
            onNodeHover={handleNodeHover}
            onBackgroundClick={handleBackgroundClick}
            enableZoomInteraction={spec.zoom_enabled !== false}
            enableNodeDrag={spec.interactive !== false}
            enablePointerInteraction={spec.interactive !== false}
            minZoom={0.3}
            maxZoom={8}
          />
        )}

        {/* Selected node detail panel */}
        {selectedNode && (
          <NodeDetailPanel
            node={selectedNode}
            onClose={() => setSelectedNode(null)}
          />
        )}

        {/* Legend */}
        <Legend typeColorIndex={typeColorIndex} />
      </div>

      {/* Footer stats */}
      <p className="text-[10px] text-zinc-400">
        {extracted.nodes.length} node{extracted.nodes.length !== 1 ? "s" : ""} ·{" "}
        {extracted.links.length} edge{extracted.links.length !== 1 ? "s" : ""}
        {isTruncated && (
          <span className="ml-1 text-amber-500">
            · Showing {extracted.nodes.length} of {extracted.totalNodes} nodes
          </span>
        )}
      </p>
    </div>
  );
});
