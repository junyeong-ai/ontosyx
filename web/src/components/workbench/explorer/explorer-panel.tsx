"use client";

import { memo, useCallback, useMemo, useRef, useState } from "react";
import { useTranslations } from "next-intl";
import { useVirtualizer } from "@tanstack/react-virtual";
import { HugeiconsIcon } from "@hugeicons/react";
import { ArrowDown01Icon, ArrowRight01Icon, Search01Icon } from "@hugeicons/core-free-icons";
import { useAppStore, selectStateSelectedNodeId, selectStateSelectedEdgeId } from "@/lib/store";
import { cn } from "@/lib/cn";
import { Tooltip } from "@/components/ui/tooltip";
import { SearchInput } from "@/components/ui/form-input";
import type { QualityGap, NodeTypeDef, EdgeTypeDef } from "@/types/api";
import { arr } from "@/lib/ir-collections";
import { localizeQualityGapIssue } from "@/lib/quality-gap-text";

// ---------------------------------------------------------------------------
// Explorer — node/edge list with search and quality indicators
// ---------------------------------------------------------------------------

type NodeLayerLabel = (
  key: string,
  vars?: Record<string, string | number | Date>,
) => string;

/** Determine the visual layer for a node (priority: problematic > suggested > asserted > inferred) */
function nodeLayerTooltip(
  t: NodeLayerLabel,
  sourceTable: string | undefined | null,
  highGapCount: number,
  isAdded: boolean,
): string {
  if (highGapCount > 0) return t("layerHighGap");
  if (isAdded) return t("layerAdded");
  if (sourceTable) return `${t("layerEnhanced")} (${sourceTable})`;
  return t("layerLegacy");
}

function nodeLayerColor(
  sourceTable: string | undefined | null,
  highGapCount: number,
  isAdded: boolean,
): string {
  if (highGapCount > 0) return "bg-danger-solid";
  if (isAdded) return "bg-info-surface";
  if (sourceTable) return "bg-brand-solid";
  return "bg-surface-raised";
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
  /** Localised string lookup scoped to `workbench.explorer.node`. */
  tNode: NodeLayerLabel;
}

const NodeItem = memo(function NodeItem({
  node,
  selected,
  gapCount,
  highGapCount,
  isAdded,
  isModified,
  onSelect,
  tNode,
}: NodeItemProps) {
  const handleClick = useCallback(() => onSelect(node.id), [onSelect, node.id]);
  const propCount = arr(node.properties).length;

  return (
    <button type="button"
      onClick={handleClick}
      className={cn(
        "flex w-full items-center gap-2 px-4 py-1.5 text-start hover:bg-surface-raised",
        selected && "bg-brand-surface",
      )}
    >
      <Tooltip content={nodeLayerTooltip(tNode, node.source_lineage?.table, highGapCount, isAdded)}>
        <span className={cn(
          "inline-block h-2 w-2 rounded-full",
          nodeLayerColor(node.source_lineage?.table, highGapCount, isAdded),
        )} />
      </Tooltip>
      <span className="flex-1 truncate text-foreground">
        {node.label}
      </span>
      <Tooltip content={tNode("propertyCount", { count: propCount })}>
        <span className="text-2xs text-foreground-muted">
          {tNode("propertyAbbrev", { count: propCount })}
        </span>
      </Tooltip>
      {isAdded && (
        <span className="rounded bg-brand-surface-strong px-1 text-2xs font-bold uppercase text-brand-foreground-strong">
          {tNode("newBadge")}
        </span>
      )}
      {isModified && (
        <span className="rounded bg-warning-surface px-1 text-2xs font-bold uppercase text-warning-foreground">
          {tNode("modifiedBadge")}
        </span>
      )}
      {gapCount > 0 && (
        <span className="flex h-3.5 w-3.5 items-center justify-center rounded-full bg-warning-surface text-2xs font-bold text-warning-foreground">
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
    <button type="button"
      onClick={handleClick}
      className={cn(
        "flex w-full items-center gap-2 px-4 py-1.5 text-start hover:bg-surface-raised",
        selected && "bg-brand-surface",
      )}
    >
      <HugeiconsIcon icon={ArrowRight01Icon} className="h-2.5 w-2.5 text-foreground-muted" size="100%" />
      <span className="flex-1 truncate text-foreground">
        <span className="text-foreground-muted">{sourceLabel}</span>
        {" → "}
        <span className="font-medium">{edge.label.replace(/_/g, " ").toLowerCase()}</span>
        {" → "}
        <span className="text-foreground-muted">{targetLabel}</span>
      </span>
      {isAdded && (
        <span className="rounded bg-brand-surface-strong px-1 text-2xs font-bold uppercase text-brand-foreground-strong">
          new
        </span>
      )}
      {isModified && (
        <span className="rounded bg-warning-surface px-1 text-2xs font-bold uppercase text-warning-foreground">
          mod
        </span>
      )}
      {gapCount > 0 && (
        <span className="flex h-3.5 w-3.5 items-center justify-center rounded-full bg-warning-surface text-2xs font-bold text-warning-foreground">
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
  const tExplorer = useTranslations("workbench.explorer");
  const tNode = useTranslations("workbench.explorer.node");
  const tGap = useTranslations("qualityGap");
  const ontology = useAppStore((s) => s.ontology);
  const selectedNodeId = useAppStore(selectStateSelectedNodeId);
  const selectedEdgeId = useAppStore(selectStateSelectedEdgeId);
  const selectOne = useAppStore((s) => s.selectOne);
  const setDesignBottomTab = useAppStore((s) => s.setDesignBottomTab);

  const lastReconcileReport = useAppStore((s) => s.lastReconcileReport);

  const handleSelectNode = useCallback((id: string) => {
    selectOne({ kind: "node", id: id });
    if (!useAppStore.getState().isInspectorOpen) useAppStore.getState().toggleInspector();
  }, [selectOne]);

  const handleSelectEdge = useCallback((id: string) => {
    selectOne({ kind: "edge", id: id });
    if (!useAppStore.getState().isInspectorOpen) useAppStore.getState().toggleInspector();
  }, [selectOne]);

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
      nodes: arr(ontology.node_types).filter(
        (n) =>
          !q ||
          n.label.toLowerCase().includes(q) ||
          arr(n.properties).some((p) => p.name.toLowerCase().includes(q)),
      ),
      edges: arr(ontology.edge_types).filter(
        (e) =>
          !q ||
          e.label.toLowerCase().includes(q) ||
          (arr(ontology.node_types).find((n) => n.id === e.source_node_id)?.label ?? "")
            .toLowerCase()
            .includes(q) ||
          (arr(ontology.node_types).find((n) => n.id === e.target_node_id)?.label ?? "")
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
      <div className="flex h-full items-center justify-center p-4 text-xs text-foreground-muted">
        {tExplorer("noOntology")}
      </div>
    );
  }

  return (
    <aside
      aria-label={tExplorer("panelAria")}
      className="flex h-full flex-col"
    >
      {/* Search */}
      <div className="border-b border-divider p-2">
        <SearchInput
          type="text"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          placeholder={tExplorer("searchPlaceholder")}
          aria-label={tExplorer("searchPlaceholder")}
          density="compact"
          leadingIcon={Search01Icon}
        />
      </div>

      {/* Legend */}
      <div className="flex flex-wrap gap-x-3 gap-y-0.5 border-b border-divider px-3 py-1.5 text-2xs text-foreground-muted">
        <span className="flex items-center gap-1"><span className="h-1.5 w-1.5 rounded-full bg-brand-solid" />{tExplorer("legend.asserted")}</span>
        <span className="flex items-center gap-1"><span className="h-1.5 w-1.5 rounded-full bg-info-surface" />{tExplorer("legend.suggested")}</span>
        <span className="flex items-center gap-1"><span className="h-1.5 w-1.5 rounded-full bg-surface-raised" />{tExplorer("legend.inferred")}</span>
        <span className="flex items-center gap-1"><span className="h-1.5 w-1.5 rounded-full bg-danger-solid" />{tExplorer("legend.problematic")}</span>
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
        tNode={tNode}
      />

      <div className="flex-shrink-0 text-xs">
        {/* Source findings (quality gaps without canvas anchor) */}
        {sourceFindings.length > 0 && (
          <>
            <button type="button"
              onClick={toggleFindings}
              className="flex w-full items-center gap-1 px-2 py-1.5 font-semibold uppercase tracking-wider text-foreground-muted hover:bg-surface-raised"
            >
              {findingsOpen ? <HugeiconsIcon icon={ArrowDown01Icon} className="h-3 w-3" size="100%" /> : <HugeiconsIcon icon={ArrowRight01Icon} className="h-3 w-3" size="100%" />}
              {tExplorer("sourceFindings", { count: sourceFindings.length })}
            </button>
            {findingsOpen && (
              <>
                {sourceFindings.map((gap, i) => (
                  <div
                    key={i}
                    className="flex items-start gap-2 px-4 py-1.5 text-2xs text-foreground-muted"
                  >
                    <span
                      className={cn(
                        "mt-0.5 h-1.5 w-1.5 rounded-full",
                        gap.severity === "high" ? "bg-danger-solid" : "bg-warning-foreground",
                      )}
                    />
                    <span>{localizeQualityGapIssue(gap, tGap)}</span>
                  </div>
                ))}
                <button type="button"
                  onClick={viewInQualityReport}
                  className="w-full px-4 py-1 text-start text-2xs font-medium text-concept-foreground hover:text-concept-foreground hover:bg-surface-raised"
                >
                  {tExplorer("viewInQualityReport")}
                </button>
              </>
            )}
          </>
        )}
      </div>
    </aside>
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
  /** Localised string lookup scoped to `workbench.explorer.node`. */
  tNode: NodeLayerLabel;
}

function VirtualizedTree({
  nodes, edges, nodesOpen, edgesOpen, toggleNodes, toggleEdges,
  nodeCount, edgeCount, selectedNodeId, selectedEdgeId,
  nodeGapCounts, nodeHighGapCounts, edgeGapCounts,
  diffAddedIds, diffModifiedIds, nodeLabelMap,
  onSelectNode, onSelectEdge, tNode,
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

  // `useVirtualizer` from @tanstack/virtual returns non-memoizable
  // closures (scrollRef-driven getters). React Compiler correctly
  // refuses to memoize callsites that consume it, and this rule is
  // informational — the rest of the file still benefits from the
  // compiler's optimisations. The lint will clear once @tanstack/virtual
  // ships compiler-friendly metadata.
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
                <button type="button"
                  onClick={row.section === "nodes" ? toggleNodes : toggleEdges}
                  className="flex w-full items-center gap-1 px-2 py-1.5 font-semibold uppercase tracking-wider text-foreground-muted hover:bg-surface-raised"
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
                  tNode={tNode}
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
