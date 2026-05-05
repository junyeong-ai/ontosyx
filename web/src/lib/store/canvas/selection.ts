"use client";

import { useEffect, useMemo, useRef } from "react";
import { useReactFlow, type Node, type Edge } from "@xyflow/react";

import {
  selectionPrimary,
  selectStateSelection,
  useAppStore,
} from "@/lib/store";
import {
  getNeighborhood,
  getNeighborhoodEdges,
} from "@/components/workbench/canvas/neighborhood";
import type { OntologyIR } from "@/types/api";
import { arr } from "@/lib/ir-collections";

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
 * Syncs the store's selection + neighborhood focus into ReactFlow
 * node/edge data so the renderer can apply "selected" and "dimmed"
 * styling. Multi-select is honoured — every ref in `selection.refs`
 * lights up, the most-recent ref drives pan/zoom and acts as the
 * inspector focus.
 *
 * Pan target: the viewport pans to the *primary* (most recent) ref
 * only — chasing every multi-select tick would whiplash the canvas.
 *
 * Escape exits neighborhood mode; the registry-owned `Escape`
 * handlers handle other contexts.
 */
export function useCanvasSelection(options: SelectionOptions) {
  const { ontology, setNodes, setEdges } = options;

  const selection = useAppStore(selectStateSelection);
  const neighborhoodFocus = useAppStore((s) => s.neighborhoodFocus);
  const setNeighborhoodFocus = useAppStore((s) => s.setNeighborhoodFocus);

  const { fitView } = useReactFlow();

  // Build the {selected node ids, selected edge ids} pair once per
  // selection change. Components downstream consume the sets directly;
  // the effect below diffs them against the previous render to limit
  // node/edge data churn.
  const selectionSets = useMemo<SelectionSets>(() => {
    const nodeIds = new Set<string>();
    const edgeIds = new Set<string>();
    for (const ref of selection.refs) {
      if (ref.kind === "node") nodeIds.add(ref.id);
      else if (ref.kind === "edge") edgeIds.add(ref.id);
    }
    return { nodeIds, edgeIds };
  }, [selection]);

  const primary = useMemo(() => selectionPrimary(selection), [selection]);
  const primaryNodeId = primary?.kind === "node" ? primary.id : null;
  const primaryEdgeId = primary?.kind === "edge" ? primary.id : null;

  // Neighborhood sets for dimming.
  const neighborhoodSets = useMemo<SelectionSets | null>(() => {
    if (!neighborhoodFocus || !ontology) return null;
    const nodeIds = getNeighborhood(
      ontology,
      neighborhoodFocus.nodeId,
      neighborhoodFocus.depth,
    );
    const edgeIds = getNeighborhoodEdges(ontology, nodeIds);
    return { nodeIds, edgeIds };
  }, [neighborhoodFocus, ontology]);

  // Pan/zoom to the primary selection (single ref).
  useEffect(() => {
    if (primaryNodeId) {
      fitView({
        nodes: [{ id: primaryNodeId }],
        duration: 300,
        padding: 0.3,
      });
    } else if (primaryEdgeId && ontology) {
      const edge = arr(ontology.edge_types).find(
        (e) => e.id === primaryEdgeId,
      );
      if (edge) {
        fitView({
          nodes: [
            { id: edge.source_node_id },
            { id: edge.target_node_id },
          ],
          duration: 300,
          padding: 0.3,
        });
      }
    }
  }, [primaryNodeId, primaryEdgeId, ontology, fitView]);

  // Apply selection + neighborhood dimming. Diff against the prior
  // render to avoid touching nodes that didn't change selection state.
  const prevRef = useRef<{
    selectionSets: SelectionSets | null;
    neighborhoodSets: SelectionSets | null;
  }>({ selectionSets: null, neighborhoodSets: null });

  useEffect(() => {
    const prev = prevRef.current;
    const neighborhoodChanged = prev.neighborhoodSets !== neighborhoodSets;
    const selectionChanged = prev.selectionSets !== selectionSets;

    // Compute the set of node ids whose `selected` flag may have
    // flipped this tick. Empty when both prev and current are empty
    // (skips the per-node loop on idle re-renders).
    const affectedNodeIds = new Set<string>();
    if (prev.selectionSets) {
      for (const id of prev.selectionSets.nodeIds) affectedNodeIds.add(id);
    }
    for (const id of selectionSets.nodeIds) affectedNodeIds.add(id);

    const affectedEdgeIds = new Set<string>();
    if (prev.selectionSets) {
      for (const id of prev.selectionSets.edgeIds) affectedEdgeIds.add(id);
    }
    for (const id of selectionSets.edgeIds) affectedEdgeIds.add(id);

    prevRef.current = { selectionSets, neighborhoodSets };

    if (!selectionChanged && !neighborhoodChanged) return;

    setNodes((prevNodes) =>
      prevNodes.map((n) => {
        if (n.type === "group") return n;
        if (
          !neighborhoodChanged &&
          affectedNodeIds.size > 0 &&
          !affectedNodeIds.has(n.id)
        ) {
          return n;
        }
        const isSelected = selectionSets.nodeIds.has(n.id);
        const data = n.data as Record<string, unknown>;
        if (!data) return n;
        const dimmed = neighborhoodSets
          ? !neighborhoodSets.nodeIds.has(n.id)
          : false;
        if (data.selected === isSelected && data.dimmed === dimmed) return n;
        return { ...n, data: { ...data, selected: isSelected, dimmed } };
      }),
    );
    setEdges((prevEdges) =>
      prevEdges.map((e) => {
        if (
          !neighborhoodChanged &&
          affectedEdgeIds.size > 0 &&
          !affectedEdgeIds.has(e.id)
        ) {
          return e;
        }
        const isSelected = selectionSets.edgeIds.has(e.id);
        const data = e.data as Record<string, unknown> | undefined;
        if (!data) return e;
        const dimmed = neighborhoodSets
          ? !neighborhoodSets.edgeIds.has(e.id)
          : false;
        if (data.selected === isSelected && data.dimmed === dimmed) return e;
        return {
          ...e,
          data: { ...data, selected: isSelected, dimmed },
          style: dimmed
            ? { opacity: 0.15, pointerEvents: "none" as const }
            : undefined,
        };
      }),
    );
  }, [selectionSets, neighborhoodSets, setNodes, setEdges]);

  // Escape exits neighborhood mode.
  useEffect(() => {
    if (!neighborhoodFocus) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") setNeighborhoodFocus(null);
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [neighborhoodFocus, setNeighborhoodFocus]);

  return {
    primaryNodeId,
    primaryEdgeId,
    selectionSets,
    neighborhoodSets,
  };
}
