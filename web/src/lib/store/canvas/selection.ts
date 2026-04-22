"use client";

import { useEffect, useMemo, useRef } from "react";
import { useReactFlow, type Node, type Edge } from "@xyflow/react";

import { useAppStore, selectStateSelectedNodeId, selectStateSelectedEdgeId } from "@/lib/store";
import { getNeighborhood, getNeighborhoodEdges } from "@/components/workbench/canvas/neighborhood";
import type { OntologyIR } from "@/types/api";

interface SelectionOptions {
  ontology: OntologyIR | null;
  setNodes: React.Dispatch<React.SetStateAction<Node[]>>;
  setEdges: React.Dispatch<React.SetStateAction<Edge[]>>;
}

interface SelectionSets {
  nodeIds: Set<string>;
  edgeIds: Set<string>;
}

/**
 * Syncs the store's selection + neighborhood focus into ReactFlow node/edge
 * data so the renderer can apply "selected" and "dimmed" styling.
 *
 * Also pans/zooms the viewport to the currently selected element and escapes
 * neighborhood focus on Escape keypress. Computes neighborhood sets once per
 * focus change and returns the current selection ids so the caller can wire
 * them into event handlers.
 */
export function useCanvasSelection(options: SelectionOptions) {
  const { ontology, setNodes, setEdges } = options;

  const selectedNodeId = useAppStore(selectStateSelectedNodeId);
  const selectedEdgeId = useAppStore(selectStateSelectedEdgeId);
  const neighborhoodFocus = useAppStore((s) => s.neighborhoodFocus);
  const setNeighborhoodFocus = useAppStore((s) => s.setNeighborhoodFocus);

  const { fitView } = useReactFlow();

  // Neighborhood sets for dimming
  const neighborhoodSets = useMemo<SelectionSets | null>(() => {
    if (!neighborhoodFocus || !ontology) return null;
    const nodeIds = getNeighborhood(ontology, neighborhoodFocus.nodeId, neighborhoodFocus.depth);
    const edgeIds = getNeighborhoodEdges(ontology, nodeIds);
    return { nodeIds, edgeIds };
  }, [neighborhoodFocus, ontology]);

  // Pan/zoom to selected element
  useEffect(() => {
    if (selectedNodeId) {
      fitView({ nodes: [{ id: selectedNodeId }], duration: 300, padding: 0.3 });
    } else if (selectedEdgeId && ontology) {
      const edge = ontology.edge_types.find((e) => e.id === selectedEdgeId);
      if (edge) {
        fitView({ nodes: [{ id: edge.source_node_id }, { id: edge.target_node_id }], duration: 300, padding: 0.3 });
      }
    }
  }, [selectedNodeId, selectedEdgeId, ontology, fitView]);

  // Apply selection + neighborhood dimming.
  // Track previous selection to limit updates to changed nodes only.
  const prevSelectionRef = useRef<{
    nodeId: string | null;
    edgeId: string | null;
    neighborhoodSets: SelectionSets | null;
  }>({ nodeId: null, edgeId: null, neighborhoodSets: null });

  useEffect(() => {
    const prev = prevSelectionRef.current;
    const neighborhoodChanged = prev.neighborhoodSets !== neighborhoodSets;

    // Build set of node IDs that need updating (old selection + new selection + neighborhood changes)
    const affectedNodeIds = new Set<string>();
    if (prev.nodeId) affectedNodeIds.add(prev.nodeId);
    if (selectedNodeId) affectedNodeIds.add(selectedNodeId);

    const affectedEdgeIds = new Set<string>();
    if (prev.edgeId) affectedEdgeIds.add(prev.edgeId);
    if (selectedEdgeId) affectedEdgeIds.add(selectedEdgeId);

    prevSelectionRef.current = { nodeId: selectedNodeId, edgeId: selectedEdgeId, neighborhoodSets };

    setNodes((prevNodes) =>
      prevNodes.map((n) => {
        if (n.type === "group") return n;
        if (!neighborhoodChanged && affectedNodeIds.size > 0 && !affectedNodeIds.has(n.id)) return n;
        const isSelected = n.id === selectedNodeId;
        const data = n.data as Record<string, unknown>;
        if (!data) return n;
        const dimmed = neighborhoodSets ? !neighborhoodSets.nodeIds.has(n.id) : false;
        if (data.selected === isSelected && data.dimmed === dimmed) return n;
        return { ...n, data: { ...data, selected: isSelected, dimmed } };
      }),
    );
    setEdges((prevEdges) =>
      prevEdges.map((e) => {
        if (!neighborhoodChanged && affectedEdgeIds.size > 0 && !affectedEdgeIds.has(e.id)) return e;
        const isSelected = e.id === selectedEdgeId;
        const data = e.data as Record<string, unknown> | undefined;
        if (!data) return e;
        const dimmed = neighborhoodSets ? !neighborhoodSets.edgeIds.has(e.id) : false;
        if (data.selected === isSelected && data.dimmed === dimmed) return e;
        return {
          ...e,
          data: { ...data, selected: isSelected, dimmed },
          style: dimmed ? { opacity: 0.15, pointerEvents: "none" as const } : undefined,
        };
      }),
    );
  }, [selectedNodeId, selectedEdgeId, neighborhoodSets, setNodes, setEdges]);

  // Escape exits neighborhood mode
  useEffect(() => {
    if (!neighborhoodFocus) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") setNeighborhoodFocus(null);
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [neighborhoodFocus, setNeighborhoodFocus]);

  return { selectedNodeId, selectedEdgeId, neighborhoodSets };
}
