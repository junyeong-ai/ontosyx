"use client";

import { memo, useCallback, useMemo, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { HugeiconsIcon } from "@hugeicons/react";
import { ArrowDown01Icon, ArrowRight01Icon, Search01Icon } from "@hugeicons/core-free-icons";
import { useAppStore, selectSelectedNodeId, selectSelectedEdgeId } from "@/lib/store";
import { cn } from "@/lib/cn";
import { Tooltip } from "@/components/ui/tooltip";
import type { QualityGap, NodeTypeDef, EdgeTypeDef } from "@/types/api";

// ---------------------------------------------------------------------------
// Explorer — node/edge list with search and quality indicators
// ---------------------------------------------------------------------------

/** Determine the visual layer for a node (priority: problematic > suggested > asserted > inferred) */
function nodeLayerTooltip(
  sourceTable: string | undefined | null,
  highGapCount: number,
  isAdded: boolean,
): string {
  if (highGapCount > 0) return `Problematic (${highGapCount} high-severity issue${highGapCount > 1 ? "s" : ""})`;
  if (isAdded) return "Suggested (LLM proposed)";
  if (sourceTable) return `Asserted (${sourceTable})`;
  return "Inferred";
}

function nodeLayerColor(
  sourceTable: string | undefined | null,
  highGapCount: number,
  isAdded: boolean,
): string {
  if (highGapCount > 0) return "bg-red-500";
  if (isAdded) return "bg-sky-500";
  if (sourceTable) return "bg-emerald-500";
  return "bg-zinc-300 dark:bg-zinc-600";
}

// ---------------------------------------------------------------------------
// Memoized child components
// ---------------------------------------------------------------------------

interface NodeItemProps {
  node: NodeTypeDef;
  selected: boolean;
  gapCount: number;
  highGapCount: number;
  isAdded: boolean;
  isModified: boolean;
  onSelect: (id: string) => void;
}

const NodeItem = memo(function NodeItem({
  node,
  selected,
  gapCount,
  highGapCount,
  isAdded,
  isModified,
  onSelect,
}: NodeItemProps) {
  const handleClick = useCallback(() => onSelect(node.id), [onSelect, node.id]);

  return (
    <button
      onClick={handleClick}
      className={cn(
        "flex w-full items-center gap-2 px-4 py-1.5 text-left hover:bg-zinc-50 dark:hover:bg-zinc-900",
        selected && "bg-emerald-50 dark:bg-emerald-950/30",
      )}
    >
      <Tooltip content={nodeLayerTooltip(node.source_table, highGapCount, isAdded)}>
        <span className={cn(
          "inline-block h-2 w-2 rounded-full",
          nodeLayerColor(node.source_table, highGapCount, isAdded),
        )} />
      </Tooltip>
      <span className="flex-1 truncate text-zinc-700 dark:text-zinc-300">
        {node.label}
      </span>
      <Tooltip content={`${node.properties.length} properties`}>
        <span className="text-[10px] text-zinc-400">
          {node.properties.length} props
        </span>
      </Tooltip>
      {isAdded && (
        <span className="rounded bg-emerald-100 px-1 text-[8px] font-bold uppercase text-emerald-700 dark:bg-emerald-900 dark:text-emerald-300">
          new
        </span>
      )}
      {isModified && (
        <span className="rounded bg-amber-100 px-1 text-[8px] font-bold uppercase text-amber-700 dark:bg-amber-900 dark:text-amber-300">
          mod
        </span>
      )}
      {gapCount > 0 && (
        <span className="flex h-3.5 w-3.5 items-center justify-center rounded-full bg-amber-100 text-[8px] font-bold text-amber-600">
          {gapCount}
        </span>
      )}
    </button>
  );
});

interface EdgeItemProps {
  edge: EdgeTypeDef;
  sourceLabel: string;
  targetLabel: string;
  selected: boolean;
  gapCount: number;
  isAdded: boolean;
  isModified: boolean;
  onSelect: (id: string) => void;
}

const EdgeItem = memo(function EdgeItem({
  edge,
  sourceLabel,
  targetLabel,
  selected,
  gapCount,
  isAdded,
  isModified,
  onSelect,
}: EdgeItemProps) {
  const handleClick = useCallback(() => onSelect(edge.id), [onSelect, edge.id]);

  return (
    <button
      onClick={handleClick}
      className={cn(
        "flex w-full items-center gap-2 px-4 py-1.5 text-left hover:bg-zinc-50 dark:hover:bg-zinc-900",
        selected && "bg-emerald-50 dark:bg-emerald-950/30",
      )}
    >
      <HugeiconsIcon icon={ArrowRight01Icon} className="h-2.5 w-2.5 text-zinc-400" size="100%" />
      <span className="flex-1 truncate text-zinc-700 dark:text-zinc-300">
        <span className="text-zinc-400">{sourceLabel}</span>
        {" → "}
        <span className="font-medium">{edge.label.replace(/_/g, " ").toLowerCase()}</span>
        {" → "}
        <span className="text-zinc-400">{targetLabel}</span>
      </span>
      {isAdded && (
        <span className="rounded bg-emerald-100 px-1 text-[8px] font-bold uppercase text-emerald-700 dark:bg-emerald-900 dark:text-emerald-300">
          new
        </span>
      )}
      {isModified && (
        <span className="rounded bg-amber-100 px-1 text-[8px] font-bold uppercase text-amber-700 dark:bg-amber-900 dark:text-amber-300">
          mod
        </span>
      )}
      {gapCount > 0 && (
        <span className="flex h-3.5 w-3.5 items-center justify-center rounded-full bg-amber-100 text-[8px] font-bold text-amber-600">
          {gapCount}
        </span>
      )}
    </button>
  );
});

// ---------------------------------------------------------------------------
// Main panel
// ---------------------------------------------------------------------------

export function ExplorerPanel({ gaps }: { gaps: QualityGap[] }) {
  const ontology = useAppStore((s) => s.ontology);
  const selectedNodeId = useAppStore(selectSelectedNodeId);
  const selectedEdgeId = useAppStore(selectSelectedEdgeId);
  const select = useAppStore((s) => s.select);
  const setDesignBottomTab = useAppStore((s) => s.setDesignBottomTab);

  const lastReconcileReport = useAppStore((s) => s.lastReconcileReport);

  const handleSelectNode = useCallback((id: string) => {
    select({ type: "node", nodeId: id });
    if (!useAppStore.getState().isInspectorOpen) useAppStore.getState().toggleInspector();
  }, [select]);

  const handleSelectEdge = useCallback((id: string) => {
    select({ type: "edge", edgeId: id });
    if (!useAppStore.getState().isInspectorOpen) useAppStore.getState().toggleInspector();
  }, [select]);

  const [search, setSearch] = useState("");
  const [nodesOpen, setNodesOpen] = useState(true);
  const [edgesOpen, setEdgesOpen] = useState(true);
  const [findingsOpen, setFindingsOpen] = useState(false);

  const toggleNodes = useCallback(() => setNodesOpen((v) => !v), []);
  const toggleEdges = useCallback(() => setEdgesOpen((v) => !v), []);
  const toggleFindings = useCallback(() => setFindingsOpen((v) => !v), []);

  const viewInQualityReport = useCallback(() => {
    setDesignBottomTab("workflow");
    // Ensure bottom panel is open
    const state = useAppStore.getState();
    if (!state.isBottomPanelOpen) state.toggleBottomPanel();
  }, [setDesignBottomTab]);

  const diffAddedIds = useMemo(() => {
    if (!lastReconcileReport) return new Set<string>();
    return new Set(lastReconcileReport.generated_ids.map((e) => e.id));
  }, [lastReconcileReport]);

  const diffModifiedIds = useMemo(() => {
    if (!lastReconcileReport) return new Set<string>();
    return new Set(lastReconcileReport.uncertain_matches.map((m) => m.original_id));
  }, [lastReconcileReport]);

  const filtered = useMemo(() => {
    if (!ontology) return { nodes: [], edges: [] };
    const q = search.toLowerCase();
    return {
      nodes: ontology.node_types.filter(
        (n) =>
          !q ||
          n.label.toLowerCase().includes(q) ||
          n.properties.some((p) => p.name.toLowerCase().includes(q)),
      ),
      edges: ontology.edge_types.filter(
        (e) =>
          !q ||
          e.label.toLowerCase().includes(q) ||
          (ontology.node_types.find((n) => n.id === e.source_node_id)?.label ?? "")
            .toLowerCase()
            .includes(q) ||
          (ontology.node_types.find((n) => n.id === e.target_node_id)?.label ?? "")
            .toLowerCase()
            .includes(q),
      ),
    };
  }, [ontology, search]);

  // Pre-compute gap count maps so each item lookup is O(1) instead of O(gaps)
  const { nodeGapCounts, nodeHighGapCounts, edgeGapCounts } = useMemo(() => {
    const nodeCounts = new Map<string, number>();
    const nodeHighCounts = new Map<string, number>();
    const edgeCounts = new Map<string, number>();
    for (const g of gaps) {
      const loc = g.location;
      if ("edge_id" in loc) {
        edgeCounts.set(loc.edge_id, (edgeCounts.get(loc.edge_id) ?? 0) + 1);
      } else if ("node_id" in loc) {
        nodeCounts.set(loc.node_id, (nodeCounts.get(loc.node_id) ?? 0) + 1);
        if (g.severity === "high") {
          nodeHighCounts.set(loc.node_id, (nodeHighCounts.get(loc.node_id) ?? 0) + 1);
        }
      }
    }
    return { nodeGapCounts: nodeCounts, nodeHighGapCounts: nodeHighCounts, edgeGapCounts: edgeCounts };
  }, [gaps]);

  // Pre-compute node label lookup map for edges
  const nodeLabelMap = useMemo(() => {
    if (!ontology) return new Map<string, string>();
    const map = new Map<string, string>();
    for (const n of ontology.node_types) {
      map.set(n.id, n.label);
    }
    return map;
  }, [ontology]);

  // Source findings (gaps without a node_id anchor)
  const sourceFindings = useMemo(
    () => gaps.filter((g) => "table" in g.location && !("node_id" in g.location)),
    [gaps],
  );

  if (!ontology) {
    return (
      <div className="flex h-full items-center justify-center p-4 text-xs text-zinc-400">
        No ontology
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col">
      {/* Search */}
      <div className="border-b border-zinc-200 p-2 dark:border-zinc-800">
        <div className="flex items-center gap-1.5 rounded-md border border-zinc-200 bg-zinc-50 px-2 py-1 dark:border-zinc-700 dark:bg-zinc-900">
          <HugeiconsIcon icon={Search01Icon} className="h-3 w-3 text-zinc-400" size="100%" />
          <input
            type="text"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="Search..."
            className="w-full bg-transparent text-xs text-zinc-700 outline-none placeholder:text-zinc-500 dark:text-zinc-300"
          />
        </div>
      </div>

      {/* Legend */}
      <div className="flex flex-wrap gap-x-3 gap-y-0.5 border-b border-zinc-200 px-3 py-1.5 text-[9px] text-zinc-400 dark:border-zinc-800">
        <span className="flex items-center gap-1"><span className="h-1.5 w-1.5 rounded-full bg-emerald-500" />Asserted</span>
        <span className="flex items-center gap-1"><span className="h-1.5 w-1.5 rounded-full bg-sky-500" />Suggested</span>
        <span className="flex items-center gap-1"><span className="h-1.5 w-1.5 rounded-full bg-zinc-300 dark:bg-zinc-600" />Inferred</span>
        <span className="flex items-center gap-1"><span className="h-1.5 w-1.5 rounded-full bg-red-500" />Problematic</span>
      </div>

      {/* Tree — virtualized for 1000+ node scale */}
      <VirtualizedTree
        nodes={nodesOpen ? filtered.nodes : []}
        edges={edgesOpen ? filtered.edges : []}
        nodesOpen={nodesOpen}
        edgesOpen={edgesOpen}
        toggleNodes={toggleNodes}
        toggleEdges={toggleEdges}
        nodeCount={filtered.nodes.length}
        edgeCount={filtered.edges.length}
        selectedNodeId={selectedNodeId}
        selectedEdgeId={selectedEdgeId}
        nodeGapCounts={nodeGapCounts}
        nodeHighGapCounts={nodeHighGapCounts}
        edgeGapCounts={edgeGapCounts}
        diffAddedIds={diffAddedIds}
        diffModifiedIds={diffModifiedIds}
        nodeLabelMap={nodeLabelMap}
        onSelectNode={handleSelectNode}
        onSelectEdge={handleSelectEdge}
      />

      <div className="flex-shrink-0 text-xs">
        {/* Source findings (quality gaps without canvas anchor) */}
        {sourceFindings.length > 0 && (
          <>
            <button
              onClick={toggleFindings}
              className="flex w-full items-center gap-1 px-2 py-1.5 font-semibold uppercase tracking-wider text-zinc-500 hover:bg-zinc-50 dark:hover:bg-zinc-900"
            >
              {findingsOpen ? <HugeiconsIcon icon={ArrowDown01Icon} className="h-3 w-3" size="100%" /> : <HugeiconsIcon icon={ArrowRight01Icon} className="h-3 w-3" size="100%" />}
              Source Findings ({sourceFindings.length})
            </button>
            {findingsOpen && (
              <>
                {sourceFindings.map((gap, i) => (
                  <div
                    key={i}
                    className="flex items-start gap-2 px-4 py-1.5 text-[10px] text-zinc-500"
                  >
                    <span
                      className={cn(
                        "mt-0.5 h-1.5 w-1.5 rounded-full",
                        gap.severity === "high" ? "bg-red-500" : "bg-amber-400",
                      )}
                    />
                    <span>{gap.issue}</span>
                  </div>
                ))}
                <button
                  onClick={viewInQualityReport}
                  className="w-full px-4 py-1 text-left text-[10px] font-medium text-violet-600 hover:text-violet-700 hover:bg-zinc-50 dark:text-violet-400 dark:hover:text-violet-300 dark:hover:bg-zinc-900"
                >
                  View in Quality Report →
                </button>
              </>
            )}
          </>
        )}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// VirtualizedTree — windowed rendering for 1000+ node scale
// ---------------------------------------------------------------------------

type VirtualRow =
  | { kind: "section-header"; section: "nodes" | "edges"; open: boolean; count: number }
  | { kind: "node"; node: NodeTypeDef }
  | { kind: "edge"; edge: EdgeTypeDef };

interface VirtualizedTreeProps {
  nodes: NodeTypeDef[];
  edges: EdgeTypeDef[];
  nodesOpen: boolean;
  edgesOpen: boolean;
  toggleNodes: () => void;
  toggleEdges: () => void;
  nodeCount: number;
  edgeCount: number;
  selectedNodeId: string | null;
  selectedEdgeId: string | null;
  nodeGapCounts: Map<string, number>;
  nodeHighGapCounts: Map<string, number>;
  edgeGapCounts: Map<string, number>;
  diffAddedIds: Set<string>;
  diffModifiedIds: Set<string>;
  nodeLabelMap: Map<string, string>;
  onSelectNode: (id: string) => void;
  onSelectEdge: (id: string) => void;
}

function VirtualizedTree({
  nodes, edges, nodesOpen, edgesOpen, toggleNodes, toggleEdges,
  nodeCount, edgeCount, selectedNodeId, selectedEdgeId,
  nodeGapCounts, nodeHighGapCounts, edgeGapCounts,
  diffAddedIds, diffModifiedIds, nodeLabelMap,
  onSelectNode, onSelectEdge,
}: VirtualizedTreeProps) {
  const parentRef = useRef<HTMLDivElement>(null);

  const rows = useMemo<VirtualRow[]>(() => {
    const result: VirtualRow[] = [];
    result.push({ kind: "section-header", section: "nodes", open: nodesOpen, count: nodeCount });
    if (nodesOpen) {
      for (const node of nodes) {
        result.push({ kind: "node", node });
      }
    }
    result.push({ kind: "section-header", section: "edges", open: edgesOpen, count: edgeCount });
    if (edgesOpen) {
      for (const edge of edges) {
        result.push({ kind: "edge", edge });
      }
    }
    return result;
  }, [nodes, edges, nodesOpen, edgesOpen, nodeCount, edgeCount]);

  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 30,
    overscan: 20,
  });

  return (
    <div ref={parentRef} className="flex-1 overflow-auto text-xs">
      <div style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
        {virtualizer.getVirtualItems().map((virtualRow) => {
          const row = rows[virtualRow.index];
          return (
            <div
              key={virtualRow.key}
              style={{
                position: "absolute",
                top: 0,
                left: 0,
                width: "100%",
                height: virtualRow.size,
                transform: `translateY(${virtualRow.start}px)`,
              }}
            >
              {row.kind === "section-header" ? (
                <button
                  onClick={row.section === "nodes" ? toggleNodes : toggleEdges}
                  className="flex w-full items-center gap-1 px-2 py-1.5 font-semibold uppercase tracking-wider text-zinc-500 hover:bg-zinc-50 dark:hover:bg-zinc-900"
                >
                  <HugeiconsIcon
                    icon={row.open ? ArrowDown01Icon : ArrowRight01Icon}
                    className="h-3 w-3"
                    size="100%"
                  />
                  {row.section === "nodes" ? "Nodes" : "Edges"} ({row.count})
                </button>
              ) : row.kind === "node" ? (
                <NodeItem
                  node={row.node}
                  selected={selectedNodeId === row.node.id}
                  gapCount={nodeGapCounts.get(row.node.id) ?? 0}
                  highGapCount={nodeHighGapCounts.get(row.node.id) ?? 0}
                  isAdded={diffAddedIds.has(row.node.id)}
                  isModified={diffModifiedIds.has(row.node.id)}
                  onSelect={onSelectNode}
                />
              ) : (
                <EdgeItem
                  edge={row.edge}
                  sourceLabel={nodeLabelMap.get(row.edge.source_node_id) ?? "?"}
                  targetLabel={nodeLabelMap.get(row.edge.target_node_id) ?? "?"}
                  selected={selectedEdgeId === row.edge.id}
                  gapCount={edgeGapCounts.get(row.edge.id) ?? 0}
                  isAdded={diffAddedIds.has(row.edge.id)}
                  isModified={diffModifiedIds.has(row.edge.id)}
                  onSelect={onSelectEdge}
                />
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
