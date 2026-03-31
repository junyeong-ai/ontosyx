"use client";

import { useMemo, useRef, useEffect, useState } from "react";
import type { ExpandNeighbor, GraphOverview } from "@/lib/api/queries";
import { useIsDarkMode } from "@/lib/use-dark-mode";
import {
  resolveNodeColor,
  resolveDisplayName,
} from "./graph-utils";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface FocusedNode {
  elementId: string;
  labels: string[];
  props: Record<string, unknown>;
}

export interface NvlGraphData {
  nodes: Array<{ id: string; color: string; size: number; caption: string; pinned?: boolean }>;
  rels: Array<{ id: string; from: string; to: string; caption: string; color: string; width: number; type: string }>;
  legendLabels: string[];
}

export interface ExploreGraphViewProps {
  focusedNode: FocusedNode | null;
  neighbors: ExpandNeighbor[];
  schemaOverview: GraphOverview | null;
  onNodeClick: (nodeId: string) => void;
}

// ---------------------------------------------------------------------------
// Data builders
// ---------------------------------------------------------------------------

function buildNeighborhoodGraph(focusedNode: FocusedNode, neighbors: ExpandNeighbor[], edgeColor: string): NvlGraphData {
  const focusedLabel = focusedNode.labels[0] || "Node";
  const nodeMap = new Map<string, NvlGraphData["nodes"][0]>();
  const labelSet = new Set<string>();

  labelSet.add(focusedLabel);
  nodeMap.set(focusedNode.elementId, {
    id: focusedNode.elementId,
    color: resolveNodeColor(focusedLabel, true),
    size: 35,
    caption: resolveDisplayName(focusedNode.props, focusedLabel),
    pinned: true,
  });

  for (const n of neighbors) {
    if (!nodeMap.has(n.element_id)) {
      const nLabel = n.labels[0] || "Node";
      labelSet.add(nLabel);
      nodeMap.set(n.element_id, {
        id: n.element_id,
        color: resolveNodeColor(nLabel, false),
        size: 22,
        caption: resolveDisplayName(n.props, nLabel),
        pinned: false,
      });
    }
  }

  const rels = neighbors.map((n, i) => ({
    id: `rel-${i}`,
    from: n.direction === "outgoing" ? focusedNode.elementId : n.element_id,
    to: n.direction === "outgoing" ? n.element_id : focusedNode.elementId,
    caption: n.relationship_type || "",
    color: edgeColor,
    width: 1,
    type: n.relationship_type || "RELATED_TO",
  }));

  return { nodes: Array.from(nodeMap.values()), rels, legendLabels: Array.from(labelSet) };
}

function buildSchemaGraph(overview: GraphOverview, edgeColor: string): NvlGraphData {
  const labelSet = new Set<string>();
  const maxCount = Math.max(...overview.labels.map((l) => l.count), 1);

  const nodes = overview.labels.map((l) => {
    labelSet.add(l.label);
    const sizeScale = Math.log10(l.count + 1) / Math.log10(maxCount + 1);
    return {
      id: `schema:${l.label}`,
      color: resolveNodeColor(l.label, false),
      size: 20 + sizeScale * 30,
      caption: `${l.label}\n(${l.count.toLocaleString()})`,
      pinned: false,
    };
  });

  const rels = overview.relationships.map((r, i) => ({
    id: `schema-rel-${i}`,
    from: `schema:${r.from_label}`,
    to: `schema:${r.to_label}`,
    caption: r.rel_type,
    color: edgeColor,
    width: Math.max(1, Math.min(3, Math.log10(r.count + 1))),
    type: r.rel_type,
  }));

  const nodeIds = new Set(nodes.map((n) => n.id));
  const validRels = rels.filter((r) => nodeIds.has(r.from) && nodeIds.has(r.to));

  return { nodes, rels: validRels, legendLabels: Array.from(labelSet) };
}

// ---------------------------------------------------------------------------
// NVL Graph Component with interaction handlers
// ---------------------------------------------------------------------------

export function ExploreGraphView({ focusedNode, neighbors, schemaOverview, onNodeClick }: ExploreGraphViewProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const nvlRef = useRef<any>(null);
  const interactionsRef = useRef<any[]>([]);
  const [nvlReady, setNvlReady] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const isDark = useIsDarkMode();

  const isSchemaMode = !focusedNode && !!schemaOverview && schemaOverview.labels.length > 0;

  // Stable refs to avoid stale closures in interaction callbacks
  const onNodeClickRef = useRef(onNodeClick);
  onNodeClickRef.current = onNodeClick;
  const isSchemaModeRef = useRef(isSchemaMode);
  isSchemaModeRef.current = isSchemaMode;
  const focusedNodeRef = useRef(focusedNode);
  focusedNodeRef.current = focusedNode;

  const edgeColor = isDark ? "#52525b" : "#a1a1aa";

  const graphData = useMemo<NvlGraphData>(() => {
    if (focusedNode) return buildNeighborhoodGraph(focusedNode, neighbors, edgeColor);
    if (schemaOverview && schemaOverview.labels.length > 0) return buildSchemaGraph(schemaOverview, edgeColor);
    return { nodes: [], rels: [], legendLabels: [] };
  }, [focusedNode, neighbors, schemaOverview, edgeColor]);

  const { nodes, rels, legendLabels } = graphData;

  // Initialize NVL + interaction handlers
  useEffect(() => {
    if (!containerRef.current || nodes.length === 0) {
      setNvlReady(false);
      return;
    }

    let nvl: any = null;
    let destroyed = false;
    const container = containerRef.current;

    // Dynamic import both NVL base and interaction handlers
    Promise.all([
      import("@neo4j-nvl/base"),
      import("@neo4j-nvl/interaction-handlers"),
    ]).then(([baseMod, interactionMod]) => {
      if (destroyed || !container) return;

      const NVLClass = baseMod.NVL ?? baseMod.default;
      if (!NVLClass) {
        setError("NVL class not found");
        return;
      }

      try {
        nvl = new NVLClass(
          container,
          nodes,
          rels,
          {
            initialZoom: 1.0,
            minZoom: 0.2,
            maxZoom: 10,
            renderer: "canvas",
            layout: "forceDirected",
            disableWebWorkers: true,
            disableTelemetry: true,
          },
        );

        // Register interaction handlers
        const interactions: any[] = [];

        // Zoom (mouse wheel)
        if (interactionMod.ZoomInteraction) {
          interactions.push(new interactionMod.ZoomInteraction(nvl));
        }

        // Pan (mouse drag on canvas)
        if (interactionMod.PanInteraction) {
          interactions.push(new interactionMod.PanInteraction(nvl));
        }

        // Drag nodes
        if (interactionMod.DragNodeInteraction) {
          interactions.push(new interactionMod.DragNodeInteraction(nvl));
        }

        // Hover (cursor change + highlight)
        if (interactionMod.HoverInteraction) {
          interactions.push(new interactionMod.HoverInteraction(nvl));
        }

        // Click — node click triggers navigation
        if (interactionMod.ClickInteraction) {
          const clickInteraction = new interactionMod.ClickInteraction(nvl, {
            selectOnClick: true,
          });
          clickInteraction.updateCallback("onNodeClick", ((node: any) => {
            if (!node?.id) return;
            const clickedId = String(node.id);

            if (isSchemaModeRef.current) {
              onNodeClickRef.current(clickedId);
            } else {
              const fn = focusedNodeRef.current;
              if (fn && clickedId !== fn.elementId) {
                onNodeClickRef.current(clickedId);
              }
            }
          }) as any);
          interactions.push(clickInteraction);
        }

        interactionsRef.current = interactions;
        nvlRef.current = nvl;
        setNvlReady(true);
        setError(null);
      } catch (err: any) {
        console.error("NVL initialization failed:", err);
        setError(err?.message || "Graph rendering failed");
        setNvlReady(false);
      }
    }).catch((err) => {
      console.error("Failed to load NVL:", err);
      setError("Failed to load graph library");
    });

    return () => {
      destroyed = true;
      // Destroy interaction handlers
      for (const interaction of interactionsRef.current) {
        try { interaction.destroy?.(); } catch { /* ignore */ }
      }
      interactionsRef.current = [];
      // Destroy NVL instance
      try { nvl?.destroy(); } catch { /* ignore */ }
      nvlRef.current = null;
    };
  }, [nodes, rels]);

  // Empty state
  if (nodes.length === 0 && !error) {
    return (
      <div className="flex h-full items-center justify-center text-zinc-500 text-sm">
        {focusedNode ? "No neighbors found" : "Loading graph schema..."}
      </div>
    );
  }

  return (
    <div className="relative h-full w-full" style={{ backgroundColor: isDark ? "#09090b" : "#fafafa" }}>
      {/* NVL container — all mouse events handled by interaction handlers */}
      <div ref={containerRef} className="h-full w-full" />

      {/* Loading overlay */}
      {!nvlReady && !error && (
        <div className="pointer-events-none absolute inset-0 flex items-center justify-center text-zinc-500 text-sm">
          Loading graph...
        </div>
      )}

      {/* Error fallback */}
      {error && (
        <div className="pointer-events-none absolute inset-0 flex flex-col items-center justify-center gap-2 text-zinc-500 text-sm">
          <p>{error}</p>
          <p className="text-xs text-zinc-600">Try refreshing the page</p>
        </div>
      )}

      {/* Mode indicator */}
      {nvlReady && isSchemaMode && (
        <div className="pointer-events-none absolute top-3 right-3 rounded bg-white/80 px-2.5 py-1 text-[10px] text-zinc-500 shadow-sm dark:bg-zinc-900/80 dark:text-zinc-400">
          Data Model — click a node to explore
        </div>
      )}

      {/* Legend */}
      {nvlReady && (
        <div className="pointer-events-none absolute top-3 left-3 flex flex-wrap gap-2 max-w-[60%]">
          {legendLabels.map(label => (
            <div key={label} className="flex items-center gap-1 rounded-full bg-white/80 px-2 py-0.5 text-[10px] text-zinc-500 shadow-sm dark:bg-zinc-900/80 dark:text-zinc-400">
              <span
                className="h-2 w-2 rounded-full"
                style={{ backgroundColor: resolveNodeColor(label, !isSchemaMode && label === (focusedNode?.labels[0] || "")) }}
              />
              {label}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
