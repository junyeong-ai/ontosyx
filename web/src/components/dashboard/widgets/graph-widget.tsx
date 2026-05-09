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
import { useTranslations } from "next-intl";
import { useLocaleChain } from "@/hooks/use-locale-chain";
import type { ForceGraphMethods } from "react-force-graph-2d";
import type {
  QueryResult,
  WidgetSpec,
  GraphLayout,
} from "@/types/api";
import { cn } from "@/lib/cn";
import { useIsDarkMode } from "@/hooks/use-dark-mode";
import { useContainerWidth } from "@/hooks/use-container-width";
import { useDashboardTypeFilter } from "@/hooks/use-dashboard-type-filter";
import { Heading } from "@/components/ui/heading";
import { brandFill, formatValue } from "./chart-utils";
import type { GraphNodeData, FGNode, FGLink } from "./graph/graph-types";
import { DEFAULT_MAX_NODES, DARK_BG, LIGHT_BG } from "./graph/graph-constants";
import { extractGraphData } from "./graph/graph-data";
import { buildTooltipHtml, layoutToDagMode } from "./graph/graph-utils";
import { GraphDetailPanel } from "./graph/graph-detail-panel";
import { GraphLegend } from "./graph/graph-legend";

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
  /**
   * When this widget is mounted inside a dashboard grid, the parent
   * passes the dashboard id so the type-visibility legend becomes a
   * cross-widget filter — toggling "hide Person" in one widget hides
   * Person nodes in every other GraphWidget on the same dashboard.
   * Omit for standalone / query-panel mounts (falls back to local
   * widget state).
   */
  dashboardId?: string | null;
}

export const GraphWidget = memo(function GraphWidget({
  spec,
  data,
  dashboardId,
}: GraphWidgetProps) {
  const t = useTranslations("widget.graph");
  const localeChain = useLocaleChain();
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

  // Type filter — clickable legend chips hide nodes (and edges whose
  // endpoints are hidden) of the selected types. Defaults to "nothing
  // hidden" so the widget behaves identically to before until someone
  // interacts with the legend. When `dashboardId` is passed, the
  // hidden-types set is shared across every GraphWidget mounted inside
  // the same dashboard (cross-widget filter).
  const typeFilter = useDashboardTypeFilter<
    (typeof extracted.nodes)[number],
    (typeof extracted.links)[number]
  >({
    dashboardId,
    allTypes: useMemo(() => Array.from(typeColorIndex.keys()), [typeColorIndex]),
    getNodeType: (n) => n.type ?? "default",
    getEdgeSource: (e) => (typeof e.source === "string" ? e.source : (e.source as { id: string }).id),
    getEdgeTarget: (e) => (typeof e.target === "string" ? e.target : (e.target as { id: string }).id),
  });

  // react-force-graph expects { nodes, links } — our custom fields survive
  // because NodeObject/LinkObject have [others: string]: any. Filter here
  // so the force simulation only simulates what the user wants to see.
  const graphData = useMemo(() => {
    const visibleNodes = typeFilter.filterNodes(extracted.nodes);
    const visibleIds = new Set(visibleNodes.map((n) => n.id));
    const visibleLinks = typeFilter.filterEdges(extracted.links, visibleIds);
    return { nodes: visibleNodes, links: visibleLinks };
  }, [extracted, typeFilter]);

  const dagMode = layoutToDagMode(layout);
  const isDirected = edgeConfig?.directed ?? true;
  const isTruncated = extracted.totalNodes > maxNodes;

  // Zoom to fit after initial render
  useEffect(() => {
    const timer = setTimeout(() => {
      graphRef.current?.zoomToFit(400, 40);
    }, 500);
    return () => clearTimeout(timer);
  }, []);

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
          ? brandFill(isDark)
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
        .map(([k, v]) => `${k}: ${formatValue(v, localeChain)}`)
        .join("\n");
      return props ? `${gl.label}\n${props}` : gl.label;
    },
    [localeChain],
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
      return buildTooltipHtml(gn, nodeConfig?.tooltip_fields, localeChain);
    },
    [nodeConfig?.tooltip_fields, localeChain],
  );

  // Empty state
  if (!data.rows.length || extracted.nodes.length === 0) {
    return (
      <div className="flex h-48 items-center justify-center rounded-lg border border-dashed border-divider">
        <p className="text-xs text-foreground-muted">{t("noData")}</p>
      </div>
    );
  }

  return (
    <div className="space-y-1.5">
      {spec.title && (
        <Heading level={4} size={6}>
          {spec.title}
        </Heading>
      )}
      <div
        ref={containerRef}
        className={cn(
          "relative overflow-hidden rounded-lg border",
          "border-divider",
          "bg-surface-base",
        )}
        role="figure"
        aria-label={spec.title ? t("ariaLabelWithTitle", { title: spec.title }) : t("ariaLabel")}
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
          <GraphDetailPanel
            node={selectedNode}
            onClose={() => setSelectedNode(null)}
          />
        )}

        <GraphLegend
          typeColorIndex={typeColorIndex}
          hiddenTypes={typeFilter.hiddenTypes}
          onToggleType={typeFilter.toggle}
        />
      </div>

      {/* Footer stats */}
      <p className="text-2xs text-foreground-muted">
        {t("nodesEdges", {
          nodes: graphData.nodes.length,
          edges: graphData.links.length,
        })}
        {typeFilter.isAnyHidden && (
          <span className="ms-1 text-foreground-muted">
            {t("hiddenCount", { count: extracted.nodes.length - graphData.nodes.length })}
          </span>
        )}
        {isTruncated && (
          <span className="ms-1 text-warning-foreground">
            {t("showingTruncated", { shown: extracted.nodes.length, total: extracted.totalNodes })}
          </span>
        )}
      </p>
    </div>
  );
});
