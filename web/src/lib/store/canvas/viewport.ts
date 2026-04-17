"use client";

import { useEffect, useMemo, useRef } from "react";

import { useAppStore } from "@/lib/store";
import {
  buildFlowElements,
  buildGapMap,
  computeAutoGroups,
} from "@/components/workbench/canvas/canvas-helpers";
import type { OntologyIR, QualityGap } from "@/types/api";

/**
 * Computes a deterministic topology signature for the ontology.
 *
 * Used to detect when node/edge labels change (as opposed to data-only
 * changes like gaps or highlights) so layout and auto-grouping can be
 * re-applied selectively.
 */
function useTopologySignature(ontology: OntologyIR | null): string {
  return useMemo(() => {
    if (!ontology) return "";
    const labelById = new Map(ontology.node_types.map((n) => [n.id, n.label]));
    const nodeLabels = ontology.node_types.map((n) => n.label).sort();
    const edgeSigs = ontology.edge_types
      .map((e) => {
        const src = labelById.get(e.source_node_id) ?? e.source_node_id;
        const tgt = labelById.get(e.target_node_id) ?? e.target_node_id;
        return `E:${e.label}:${src}:${tgt}`;
      })
      .sort();
    return `topo:${nodeLabels.join(",")}|${edgeSigs.join(",")}`;
  }, [ontology]);
}

/**
 * Derives the ReactFlow node/edge elements from the current ontology state
 * and runs one-shot auto-grouping for large ontologies.
 *
 * Returned values:
 *  - `flowElements`: pre-built nodes+edges (null when no ontology)
 *  - `topologySignature`: string key that changes only with structural shape
 */
export function useCanvasViewport(gaps: QualityGap[]) {
  const ontology = useAppStore((s) => s.ontology);
  const highlightedBindings = useAppStore((s) => s.highlightedBindings);
  const lastReconcileReport = useAppStore((s) => s.lastReconcileReport);
  const activeDiffOverlay = useAppStore((s) => s.activeDiffOverlay);
  const nodeGroups = useAppStore((s) => s.nodeGroups);
  const restoreNodeGroups = useAppStore((s) => s.restoreNodeGroups);

  const gapMap = useMemo(() => buildGapMap(gaps), [gaps]);

  const flowElements = useMemo(() => {
    if (!ontology) return null;
    return buildFlowElements(ontology, gapMap, highlightedBindings, lastReconcileReport, nodeGroups, activeDiffOverlay);
  }, [ontology, gapMap, highlightedBindings, lastReconcileReport, nodeGroups, activeDiffOverlay]);

  const topologySignature = useTopologySignature(ontology);

  // Auto-group large ontologies once per topology, only when no groups exist.
  const autoGroupAppliedRef = useRef<string>("");
  useEffect(() => {
    if (!ontology) return;
    if (autoGroupAppliedRef.current === topologySignature) return;
    if (Object.keys(nodeGroups).length > 0) {
      autoGroupAppliedRef.current = topologySignature;
      return;
    }
    const autoGroups = computeAutoGroups(ontology);
    if (Object.keys(autoGroups).length > 0) {
      restoreNodeGroups(autoGroups);
    }
    autoGroupAppliedRef.current = topologySignature;
  }, [ontology, topologySignature, nodeGroups, restoreNodeGroups]);

  return { flowElements, topologySignature };
}
