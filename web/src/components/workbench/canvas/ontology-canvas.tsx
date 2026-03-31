"use client";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  ReactFlow,
  Background,
  Controls,
  MiniMap,
  useNodesState,
  useEdgesState,
  useReactFlow,
  ReactFlowProvider,
  type Node,
  type Edge,
  type NodeMouseHandler,
  type EdgeMouseHandler,
  type OnNodesChange,
  type OnEdgesChange,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";

import { useAppStore, selectSelectedNodeId, selectSelectedEdgeId } from "@/lib/store";
import { applyOntologyCommands } from "@/lib/api";
import { useCanvasState } from "./use-canvas-state";
import { handleSchemaExport } from "@/lib/export-utils";
import type { ExportFormat } from "@/lib/export-utils";
import { SchemaNode } from "./schema-node";
import { SchemaEdge } from "./schema-edge";
import { GroupNode } from "./node-group";
import { CommandBar, DiffOverlayBar, VersionDiffBar } from "./command-bar";
import { ContextMenu } from "./context-menu";
import { CommandPalette } from "./command-palette";
import { PerspectiveSwitcher } from "./perspective-switcher";
import { getNeighborhood, getNeighborhoodEdges } from "./neighborhood";
import { NeighborhoodToolbar } from "./neighborhood-toolbar";
import { buildGapMap, buildFlowElements, exportCanvasImage, computeAutoGroups } from "./canvas-helpers";
import { useCanvasLayout } from "./use-canvas-layout";
import { CanvasSkeleton } from "./canvas-skeleton";
import { useCanvasKeyboard } from "./use-canvas-keyboard";
import { useCanvasContextMenu } from "./use-canvas-context-menu";
import { useClickOutside } from "@/lib/use-click-outside";
import { toast } from "sonner";
import type { OntologyIR, QualityGap } from "@/types/api";
import { HugeiconsIcon } from "@hugeicons/react";
import { PencilEdit01Icon, DatabaseIcon, Upload04Icon } from "@hugeicons/core-free-icons";

// ---------------------------------------------------------------------------
// Node/edge type registration
// ---------------------------------------------------------------------------

const nodeTypes = { schema: SchemaNode, group: GroupNode };
const edgeTypes = { schema: SchemaEdge };

// ---------------------------------------------------------------------------
// Canvas inner component (needs ReactFlowProvider above)
// ---------------------------------------------------------------------------

function CanvasInner({ gaps }: { gaps: QualityGap[] }) {
  const {
    ontology,
    select,
    clearSelection,
    highlightedBindings,
    setHighlightedBindings,
    lastReconcileReport,
    activeDiffOverlay,
    nodeGroups,
    restoreNodeGroups,
    neighborhoodFocus,
    setNeighborhoodFocus,
    applyCommand,
    setActiveProject,
    setOntology,
  } = useCanvasState();
  const selectedNodeId = useAppStore(selectSelectedNodeId);
  const selectedEdgeId = useAppStore(selectSelectedEdgeId);

  const { fitView } = useReactFlow();
  const [nodes, setNodes, onNodesChange] = useNodesState<Node>([]);
  const [edges, setEdges, onEdgesChange] = useEdgesState<Edge>([]);

  // Export dropdown state
  const [isExportOpen, setIsExportOpen] = useState(false);
  const exportRef = useRef<HTMLDivElement>(null);
  // Command palette state
  const [isPaletteOpen, setIsPaletteOpen] = useState(false);

  // Close export dropdown on outside click
  const closeExport = useCallback(() => setIsExportOpen(false), []);
  useClickOutside(exportRef, closeExport, isExportOpen);

  // Pre-compute gap map
  const gapMap = useMemo(() => buildGapMap(gaps), [gaps]);

  // Build flow elements when ontology, gaps, bindings, or diff changes
  const flowElements = useMemo(() => {
    if (!ontology) return null;
    return buildFlowElements(ontology, gapMap, highlightedBindings, lastReconcileReport, nodeGroups, activeDiffOverlay);
  }, [ontology, gapMap, highlightedBindings, lastReconcileReport, nodeGroups, activeDiffOverlay]);

  // Topology signature: deterministic string from structural shape
  const topologySignature = useMemo(() => {
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

  // Auto-group large ontologies (50+ nodes, only when no groups exist)
  const autoGroupAppliedRef = useRef<string>("");
  useEffect(() => {
    if (!ontology) return;
    const sig = topologySignature;
    // Only auto-group once per topology, and only when no groups exist
    if (autoGroupAppliedRef.current === sig) return;
    if (Object.keys(nodeGroups).length > 0) {
      autoGroupAppliedRef.current = sig;
      return;
    }
    const autoGroups = computeAutoGroups(ontology);
    if (Object.keys(autoGroups).length > 0) {
      restoreNodeGroups(autoGroups);
    }
    autoGroupAppliedRef.current = sig;
  }, [ontology, topologySignature, nodeGroups, restoreNodeGroups]);

  // Neighborhood sets for dimming
  const neighborhoodSets = useMemo(() => {
    if (!neighborhoodFocus || !ontology) return null;
    const nodeIds = getNeighborhood(ontology, neighborhoodFocus.nodeId, neighborhoodFocus.depth);
    const edgeIds = getNeighborhoodEdges(ontology, nodeIds);
    return { nodeIds, edgeIds };
  }, [neighborhoodFocus, ontology]);

  // --- Layout hook ---
  const { onNodeDragStop, runAutoLayout, layoutReady } = useCanvasLayout(
    flowElements,
    topologySignature,
    setNodes,
    setEdges,
  );

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

  // Apply selection + neighborhood dimming
  // Track previous selection to limit updates to changed nodes only
  const prevSelectionRef = useRef<{ nodeId: string | null; edgeId: string | null; neighborhoodSets: typeof neighborhoodSets }>({
    nodeId: null,
    edgeId: null,
    neighborhoodSets: null,
  });

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
        // Skip nodes unaffected by selection change (unless neighborhood changed globally)
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

  // --- Action callbacks ---

  const handleSave = useCallback(async () => {
    const store = useAppStore.getState();
    if (!store.activeProject || store.commandStack.length === 0) return;
    try {
      const commands = store.commandStack.map((e) => e.command);
      const resp = await applyOntologyCommands(store.activeProject.id, {
        revision: store.activeProject.revision,
        commands,
      });
      setOntology(resp.project.ontology as OntologyIR);
      setActiveProject(resp.project);
      toast.success("Ontology saved");
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Failed to save");
    }
  }, [setOntology, setActiveProject]);

  const deleteSelected = useCallback(() => {
    const store = useAppStore.getState();
    const nodeId = store.selection.type === "node" ? store.selection.nodeId : null;
    const edgeId = store.selection.type === "edge" ? store.selection.edgeId : null;
    if (nodeId) {
      applyCommand({ op: "delete_node", node_id: nodeId });
      clearSelection();
      toast.success("Node deleted");
    } else if (edgeId) {
      applyCommand({ op: "delete_edge", edge_id: edgeId });
      clearSelection();
      toast.success("Edge deleted");
    }
  }, [applyCommand, clearSelection]);

  const selectAllNodes = useCallback(() => {
    if (ontology && ontology.node_types.length > 0) {
      select({ type: "node", nodeId: ontology.node_types[0].id });
    }
  }, [ontology, select]);

  const handleExport = useCallback(async (format: ExportFormat) => {
    if (!ontology) return;
    await handleSchemaExport(ontology, format);
  }, [ontology]);

  const deselectAll = useCallback(() => {
    clearSelection();
    setHighlightedBindings(null);
    setIsPaletteOpen(false);
    setIsExportOpen(false);
    setNeighborhoodFocus(null);
  }, [clearSelection, setHighlightedBindings, setNeighborhoodFocus]);

  // --- Keyboard hook ---
  const { paletteCommands: getPaletteCommands } = useCanvasKeyboard({
    handleSave,
    deleteSelected,
    runAutoLayout: () => runAutoLayout(nodes, edges),
    selectAllNodes,
    deselectAll,
    exportPng: () => exportCanvasImage(nodes, "png", ontology?.name ?? "ontology"),
    exportSvg: () => exportCanvasImage(nodes, "svg", ontology?.name ?? "ontology"),
    setIsPaletteOpen,
  });
  const memoizedPaletteCommands = useMemo(() => getPaletteCommands(), [getPaletteCommands]);

  // --- Context menu hook ---
  const {
    contextMenu,
    closeContextMenu,
    handleNodeContextMenu,
    handleEdgeContextMenu,
    nodeContextMenuItems,
    edgeContextMenuItems,
  } = useCanvasContextMenu(ontology);

  // --- Event handlers ---

  const onNodeClick: NodeMouseHandler = useCallback(
    (_event, node) => {
      closeContextMenu();
      if (node.type === "group") return;
      select({ type: "node", nodeId: node.id });
    },
    [select, closeContextMenu],
  );

  const onNodeDoubleClick: NodeMouseHandler = useCallback(
    (_event, node) => {
      if (node.type === "group") return;
      setNeighborhoodFocus({ nodeId: node.id, depth: 1 });
    },
    [setNeighborhoodFocus],
  );

  const onEdgeClick: EdgeMouseHandler = useCallback(
    (_event, edge) => {
      closeContextMenu();
      select({ type: "edge", edgeId: edge.id });
    },
    [select, closeContextMenu],
  );

  const onPaneClick = useCallback(() => {
    closeContextMenu();
    clearSelection();
    setHighlightedBindings(null);
  }, [clearSelection, setHighlightedBindings, closeContextMenu]);

  // Callback for perspective switcher
  const applyPositions = useCallback(
    (positions: Record<string, { x: number; y: number }>) => {
      setNodes((prev) =>
        prev.map((n) => {
          const pos = positions[n.id];
          return pos ? { ...n, position: { x: pos.x, y: pos.y } } : n;
        }),
      );
    },
    [setNodes],
  );

  if (!ontology) {
    const store = useAppStore.getState();
    const hasProject = !!store.activeProject;

    // Project exists but ontology not yet designed — show contextual guidance
    if (hasProject) {
      return (
        <div className="flex h-full flex-col items-center justify-center gap-4 p-8">
          <div className="flex h-14 w-14 items-center justify-center rounded-full bg-emerald-50 dark:bg-emerald-950/30">
            <HugeiconsIcon icon={PencilEdit01Icon} className="h-6 w-6 text-emerald-500" size="100%" />
          </div>
          <div className="text-center">
            <h2 className="text-lg font-semibold text-zinc-800 dark:text-zinc-200">Ready to Design</h2>
            <p className="mt-1.5 max-w-md text-sm text-zinc-500">
              Review the analysis in the Workflow panel below, then click <strong>Design Ontology</strong> to generate your knowledge graph schema.
            </p>
          </div>
          <button
            onClick={() => {
              const s = useAppStore.getState();
              s.setDesignBottomTab("workflow");
              if (!s.isBottomPanelOpen) s.toggleBottomPanel();
            }}
            className="rounded-lg bg-emerald-600 px-4 py-2 text-xs font-medium text-white transition-colors hover:bg-emerald-700"
          >
            Open Workflow Panel
          </button>
        </div>
      );
    }

    // No project at all — show create/import options
    return (
      <div className="flex h-full flex-col items-center justify-center gap-6 p-8">
        <div className="flex h-14 w-14 items-center justify-center rounded-full bg-emerald-50 dark:bg-emerald-950/30">
          <HugeiconsIcon icon={PencilEdit01Icon} className="h-6 w-6 text-emerald-500" size="100%" />
        </div>
        <div className="text-center">
          <h2 className="text-lg font-semibold text-zinc-800 dark:text-zinc-200">Start Designing</h2>
          <p className="mt-1.5 max-w-md text-sm text-zinc-500">
            Create a project from a data source or import an existing ontology to begin designing your knowledge graph.
          </p>
        </div>
        <div className="flex items-center gap-4">
          <button
            onClick={() => {
              const s = useAppStore.getState();
              s.setDesignBottomTab("workflow");
              if (!s.isBottomPanelOpen) s.toggleBottomPanel();
            }}
            className="flex flex-col items-center gap-2 rounded-xl border border-zinc-200 bg-white p-5 text-center transition-all hover:border-emerald-300 hover:shadow-md dark:border-zinc-700 dark:bg-zinc-900 dark:hover:border-emerald-700"
          >
            <HugeiconsIcon icon={DatabaseIcon} className="h-5 w-5 text-emerald-600 dark:text-emerald-400" size="100%" />
            <span className="text-xs font-medium text-zinc-700 dark:text-zinc-300">Create Project</span>
            <span className="text-[10px] text-zinc-400">Database, CSV, JSON, or code repo</span>
          </button>
          <span className="text-xs text-zinc-400">or</span>
          <button
            onClick={() => {
              const fileInput = document.querySelector('input[type="file"][accept=".json,.ttl,.owl"]') as HTMLInputElement;
              fileInput?.click();
            }}
            className="flex flex-col items-center gap-2 rounded-xl border border-zinc-200 bg-white p-5 text-center transition-all hover:border-emerald-300 hover:shadow-md dark:border-zinc-700 dark:bg-zinc-900 dark:hover:border-emerald-700"
          >
            <HugeiconsIcon icon={Upload04Icon} className="h-5 w-5 text-indigo-600 dark:text-indigo-400" size="100%" />
            <span className="text-xs font-medium text-zinc-700 dark:text-zinc-300">Import Ontology</span>
            <span className="text-[10px] text-zinc-400">JSON, OWL, or Turtle file</span>
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="relative h-full w-full">
      <CanvasSkeleton visible={!layoutReady} />
      <ReactFlow
        nodes={nodes}
        edges={edges}
        onNodesChange={onNodesChange as OnNodesChange}
        onEdgesChange={onEdgesChange as OnEdgesChange}
        onNodeClick={onNodeClick}
        onEdgeClick={onEdgeClick}
        onNodeDragStop={onNodeDragStop}
        onNodeDoubleClick={onNodeDoubleClick}
        onNodeContextMenu={handleNodeContextMenu}
        onEdgeContextMenu={handleEdgeContextMenu}
        onPaneClick={onPaneClick}
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
            return "#a1a1aa"; // inferred — visible in both light and dark
          }}
          maskColor="rgba(0,0,0,0.08)"
          className="!rounded-lg !border-zinc-200 !bg-white dark:!border-zinc-700 dark:!bg-zinc-900"
        />
      </ReactFlow>

      {/* Canvas toolbar — Export + Perspective (wrap on small screens) */}
      <div className="absolute right-2 top-2 z-10 flex flex-wrap items-center justify-end gap-1.5">
        <PerspectiveSwitcher
          nodes={nodes}
          topologySignature={topologySignature}
          onApplyPositions={applyPositions}
          onOpen={() => setIsExportOpen(false)}
        />
        <div ref={exportRef} className="relative">
          <button
            onClick={() => setIsExportOpen((v) => !v)}
            className="flex items-center rounded-md border border-zinc-200 bg-white px-2 py-1 text-[10px] font-medium text-zinc-600 shadow-sm transition-colors hover:bg-zinc-50 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-400 dark:hover:bg-zinc-800"
          >
            Export
          </button>
          {isExportOpen && (
            <div className="absolute right-0 top-full mt-1 min-w-[160px] rounded-lg border border-zinc-200 bg-white py-1 shadow-lg dark:border-zinc-700 dark:bg-zinc-900">
              <div className="px-3 py-1 text-[10px] font-medium uppercase tracking-wider text-zinc-400">Image</div>
              <button
                onClick={() => { setIsExportOpen(false); exportCanvasImage(nodes, "png", ontology.name); }}
                className="flex w-full items-center px-3 py-1.5 text-xs text-zinc-700 hover:bg-zinc-100 dark:text-zinc-300 dark:hover:bg-zinc-800"
              >
                PNG
              </button>
              <button
                onClick={() => { setIsExportOpen(false); exportCanvasImage(nodes, "svg", ontology.name); }}
                className="flex w-full items-center px-3 py-1.5 text-xs text-zinc-700 hover:bg-zinc-100 dark:text-zinc-300 dark:hover:bg-zinc-800"
              >
                SVG
              </button>
              <div className="my-1 h-px bg-zinc-200 dark:bg-zinc-700" />
              <div className="px-3 py-1 text-[10px] font-medium uppercase tracking-wider text-zinc-400">Schema</div>
              <button
                onClick={async () => { setIsExportOpen(false); await handleExport("json"); }}
                className="flex w-full items-center px-3 py-1.5 text-xs text-zinc-700 hover:bg-zinc-100 dark:text-zinc-300 dark:hover:bg-zinc-800"
              >
                JSON
              </button>
              <button
                onClick={async () => { setIsExportOpen(false); await handleExport("cypher"); }}
                className="flex w-full items-center px-3 py-1.5 text-xs text-zinc-700 hover:bg-zinc-100 dark:text-zinc-300 dark:hover:bg-zinc-800"
              >
                Cypher DDL
              </button>
              <button
                onClick={async () => { setIsExportOpen(false); await handleExport("mermaid"); }}
                className="flex w-full items-center px-3 py-1.5 text-xs text-zinc-700 hover:bg-zinc-100 dark:text-zinc-300 dark:hover:bg-zinc-800"
              >
                Mermaid Diagram
              </button>
              <button
                onClick={async () => { setIsExportOpen(false); await handleExport("graphql"); }}
                className="flex w-full items-center px-3 py-1.5 text-xs text-zinc-700 hover:bg-zinc-100 dark:text-zinc-300 dark:hover:bg-zinc-800"
              >
                GraphQL Schema
              </button>
              <button
                onClick={async () => { setIsExportOpen(false); await handleExport("owl"); }}
                className="flex w-full items-center px-3 py-1.5 text-xs text-zinc-700 hover:bg-zinc-100 dark:text-zinc-300 dark:hover:bg-zinc-800"
              >
                OWL/Turtle
              </button>
              <button
                onClick={async () => { setIsExportOpen(false); await handleExport("shacl"); }}
                className="flex w-full items-center px-3 py-1.5 text-xs text-zinc-700 hover:bg-zinc-100 dark:text-zinc-300 dark:hover:bg-zinc-800"
              >
                SHACL Shapes
              </button>
            </div>
          )}
        </div>
      </div>
      <NeighborhoodToolbar />
      <DiffOverlayBar />
      <VersionDiffBar />
      <CommandBar />

      {contextMenu && (
        <ContextMenu
          state={contextMenu}
          items={contextMenu.type === "node" ? nodeContextMenuItems : edgeContextMenuItems}
          onClose={closeContextMenu}
        />
      )}

      {isPaletteOpen && (
        <CommandPalette
          open={isPaletteOpen}
          onClose={() => setIsPaletteOpen(false)}
          commands={memoizedPaletteCommands}
        />
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Exported canvas with provider
// ---------------------------------------------------------------------------

export function OntologyCanvas({ gaps }: { gaps: QualityGap[] }) {
  return (
    <ReactFlowProvider>
      <CanvasInner gaps={gaps} />
    </ReactFlowProvider>
  );
}
