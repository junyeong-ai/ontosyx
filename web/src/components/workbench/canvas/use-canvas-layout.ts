"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { useReactFlow, type Node, type Edge } from "@xyflow/react";

import { useAppStore } from "@/lib/store";
import { findBestPerspective, savePerspective } from "@/lib/api";
import { computeElkLayout, type ElkLayoutPreset } from "./elk-layout";
import { useUiConfig } from "./use-ui-config";

interface FlowElements {
  nodes: Node[];
  edges: Edge[];
}

/**
 * Manages ELK auto-layout, perspective restore, and auto-save on drag.
 *
 * When topology changes (node/edge labels added/removed), attempts to load
 * a saved perspective first, falling back to ELK layout. On data-only changes
 * (gaps, rename, highlight), preserves existing positions.
 */
export function useCanvasLayout(
  flowElements: FlowElements | null,
  topologySignature: string,
  setNodes: React.Dispatch<React.SetStateAction<Node[]>>,
  setEdges: React.Dispatch<React.SetStateAction<Edge[]>>,
  preset: ElkLayoutPreset = "layered",
) {
  const uiConfig = useUiConfig();
  const uiConfigRef = useRef(uiConfig);
  // Render-phase ref sync — layout math below reads the freshest
  // uiConfig without re-firing on every field. Switch to
  // `useEffectEvent` when it leaves experimental.
  uiConfigRef.current = uiConfig;
  // Same latest-ref pattern for the active preset so the long-lived
  // effect below picks the freshest algorithm without thrashing
  // its dep list every time the user flips between hierarchical /
  // tree / radial. The preset-change effect below explicitly
  // re-runs the layout when the value differs.
  const presetRef = useRef(preset);
  presetRef.current = preset;
  const ontology = useAppStore((s) => s.ontology);
  const { fitView, setViewport, getViewport, getNodes: getFlowNodes } = useReactFlow();

  const [layoutReady, setLayoutReady] = useState(false);
  const layoutDoneRef = useRef(false);
  const prevTopologyRef = useRef<string>("");
  const layoutVersionRef = useRef(0);
  const saveTimerRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);

  // Apply ELK layout or restore saved perspective when topology changes
  useEffect(() => {
    if (!flowElements || !ontology) return;

    const topologyChanged = prevTopologyRef.current !== topologySignature;
    prevTopologyRef.current = topologySignature;

    if (topologyChanged || !layoutDoneRef.current) {
      setLayoutReady(false);
      const version = ++layoutVersionRef.current;

      findBestPerspective(ontology.id, topologySignature)
        .then((perspective) => {
          if (layoutVersionRef.current !== version) return;

          if (perspective?.positions && Object.keys(perspective.positions).length > 0) {
            const positions = perspective.positions as Record<string, { x: number; y: number }>;
            const restoredNodes = flowElements.nodes.map((n) => ({
              ...n,
              position: positions[n.id] ?? n.position,
            }));
            setNodes(restoredNodes);
            setEdges(flowElements.edges);
            layoutDoneRef.current = true;
            setLayoutReady(true);
            const vp = perspective.viewport as { x: number; y: number; zoom: number } | undefined;
            if (vp && typeof vp.x === "number" && typeof vp.y === "number" && typeof vp.zoom === "number") {
              setTimeout(() => setViewport({ x: vp.x, y: vp.y, zoom: vp.zoom }, { duration: 300 }), 50);
            } else {
              setTimeout(() => fitView({ padding: 0.15, duration: 300 }), 50);
            }
          } else {
            applyElkLayout(flowElements, version);
          }
        })
        .catch(() => {
          if (layoutVersionRef.current !== version) return;
          applyElkLayout(flowElements, version);
        });
    } else {
      // Data-only update -- preserve positions
      setNodes((prev) => {
        const newById = new Map(flowElements.nodes.map((n) => [n.id, n]));
        const updated: Node[] = [];
        for (const n of prev) {
          const updatedNode = newById.get(n.id);
          if (!updatedNode) continue;
          newById.delete(n.id);
          updated.push({ ...n, data: updatedNode.data, style: updatedNode.style });
        }
        for (const newNode of newById.values()) {
          updated.push(newNode);
        }
        return updated;
      });
      setEdges(flowElements.edges);
    }

    function applyElkLayout(elements: FlowElements, version: number) {
      const schemaNodes = elements.nodes.filter((n) => n.type === "schema");
      const groupNodes = elements.nodes.filter((n) => n.type === "group");
      computeElkLayout(schemaNodes, elements.edges, {
        preset: presetRef.current,
        uiConfig: uiConfigRef.current ?? undefined,
      }).then((result) => {
        if (layoutVersionRef.current !== version) return;
        setNodes([...groupNodes, ...result.nodes]);
        setEdges(result.edges);
        layoutDoneRef.current = true;
        setLayoutReady(true);
        setTimeout(() => fitView({ padding: 0.15, duration: 300 }), 50);
      });
    }
  }, [flowElements, ontology, topologySignature, fitView, setViewport, setNodes, setEdges]);

  // Cleanup save timer on unmount
  useEffect(() => {
    return () => {
      if (saveTimerRef.current) clearTimeout(saveTimerRef.current);
    };
  }, []);

  // Debounced auto-save perspective on node drag
  const savePerspectiveDebounced = useCallback(() => {
    if (!ontology) return;
    clearTimeout(saveTimerRef.current);
    saveTimerRef.current = setTimeout(() => {
      const currentOntology = useAppStore.getState().ontology;
      if (!currentOntology) return;
      const currentNodes = getFlowNodes();
      if (currentNodes.length === 0) return;
      const positions: Record<string, { x: number; y: number }> = {};
      for (const n of currentNodes) {
        positions[n.id] = { x: n.position.x, y: n.position.y };
      }
      const vp = getViewport();
      const groups = useAppStore.getState().nodeGroups;
      const currentProject = useAppStore.getState().activeProject;
      savePerspective({
        lineage_id: currentOntology.id,
        topology_signature: topologySignature,
        project_id: currentProject?.id,
        name: "Default",
        positions,
        viewport: { x: vp.x, y: vp.y, zoom: vp.zoom },
        is_default: true,
        filters: { groups },
      }).catch(() => { /* non-critical: perspective auto-save */ });
    }, 1500);
  }, [ontology, topologySignature, getFlowNodes, getViewport]);

  const onNodeDragStop = useCallback(
    () => savePerspectiveDebounced(),
    [savePerspectiveDebounced],
  );

  // Re-layout on preset change. The layout effect above keys on
  // `topologySignature`, so a preset flip alone wouldn't trigger a
  // re-run; we drive it through `runAutoLayout` once the user picks
  // a new algorithm so the canvas reshapes without forcing a
  // schema-change cycle.
  const lastPresetRef = useRef(preset);
  useEffect(() => {
    if (!flowElements) return;
    if (lastPresetRef.current === preset) return;
    lastPresetRef.current = preset;
    if (!layoutDoneRef.current) return;
    void (async () => {
      const schemaNodes = flowElements.nodes.filter((n) => n.type === "schema");
      const groupNodes = flowElements.nodes.filter((n) => n.type === "group");
      const result = await computeElkLayout(schemaNodes, flowElements.edges, {
        preset,
        uiConfig: uiConfigRef.current ?? undefined,
      });
      setNodes([...groupNodes, ...result.nodes]);
      setEdges(result.edges);
      setTimeout(() => fitView({ padding: 0.15, duration: 300 }), 50);
    })();
  }, [preset, flowElements, setNodes, setEdges, fitView]);

  // Run auto-layout on demand
  const runAutoLayout = useCallback(async (nodes: Node[], edges: Edge[]) => {
    if (nodes.length === 0) return;
    const schemaNodes = nodes.filter((n) => n.type === "schema");
    const groupNodes = nodes.filter((n) => n.type === "group");
    const result = await computeElkLayout(schemaNodes, edges, {
      preset: presetRef.current,
      uiConfig: uiConfigRef.current ?? undefined,
    });
    setNodes([...groupNodes, ...result.nodes]);
    setEdges(result.edges);
    setTimeout(() => fitView({ padding: 0.15, duration: 300 }), 50);
  }, [setNodes, setEdges, fitView]);

  return { onNodeDragStop, runAutoLayout, layoutReady };
}
