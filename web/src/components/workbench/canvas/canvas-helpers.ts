"use client";

import {
  getNodesBounds,
  getViewportForBounds,
  type Node,
  type Edge,
} from "@xyflow/react";
import { toPng, toSvg } from "html-to-image";
import { toast } from "sonner";

import type { NodeGroup } from "@/lib/store";
import type { SchemaNodeData, NodeLayer, DiffStatus } from "./schema-node";
import type { SchemaEdgeData } from "./schema-edge";
import type { GroupNodeData } from "./node-group";
import type { OntologyIR, OntologyDiff, QualityGap, QualityGapRef, ReconcileReport, ResolvedQueryBindings, BindingKind } from "@/types/api";

// ---------------------------------------------------------------------------
// Highlight sets
// ---------------------------------------------------------------------------

export interface HighlightSets {
  nodeIds: Set<string>;
  edgeIds: Set<string>;
  /** Best binding kind per node (priority: match > exists > path_find > chain > mutation) */
  nodeKinds: Map<string, BindingKind>;
  /** Best binding kind per edge */
  edgeKinds: Map<string, BindingKind>;
  /** Property IDs referenced per node */
  nodePropertyIds: Map<string, Set<string>>;
}

const BINDING_PRIORITY: Record<BindingKind, number> = {
  match: 0,
  exists: 1,
  path_find: 2,
  chain: 3,
  mutation: 4,
};

// ---------------------------------------------------------------------------
// Gap map
// ---------------------------------------------------------------------------

/** Pre-compute gap lookup: nodeId -> QualityGap[] */
export function buildGapMap(gaps: QualityGap[]): Map<string, QualityGap[]> {
  const map = new Map<string, QualityGap[]>();
  for (const gap of gaps) {
    const loc = gap.location as QualityGapRef;
    if ((loc.ref_type === "node" || loc.ref_type === "node_property") && loc.node_id) {
      const existing = map.get(loc.node_id);
      if (existing) {
        existing.push(gap);
      } else {
        map.set(loc.node_id, [gap]);
      }
    }
  }
  return map;
}

// ---------------------------------------------------------------------------
// Highlight / diff builders
// ---------------------------------------------------------------------------

export function buildHighlightSets(bindings: ResolvedQueryBindings | null): HighlightSets {
  const empty: HighlightSets = {
    nodeIds: new Set(),
    edgeIds: new Set(),
    nodeKinds: new Map(),
    edgeKinds: new Map(),
    nodePropertyIds: new Map(),
  };
  if (!bindings) return empty;

  const nodeIds = new Set(bindings.node_bindings.map((b) => b.node_id));
  const edgeIds = new Set(bindings.edge_bindings.map((b) => b.edge_id));

  // Best binding kind per element (lowest priority number wins)
  const nodeKinds = new Map<string, BindingKind>();
  for (const b of bindings.node_bindings) {
    const existing = nodeKinds.get(b.node_id);
    if (!existing || BINDING_PRIORITY[b.binding_kind] < BINDING_PRIORITY[existing]) {
      nodeKinds.set(b.node_id, b.binding_kind);
    }
  }

  const edgeKinds = new Map<string, BindingKind>();
  for (const b of bindings.edge_bindings) {
    const existing = edgeKinds.get(b.edge_id);
    if (!existing || BINDING_PRIORITY[b.binding_kind] < BINDING_PRIORITY[existing]) {
      edgeKinds.set(b.edge_id, b.binding_kind);
    }
  }

  // Aggregate property IDs per owner node
  const nodePropertyIds = new Map<string, Set<string>>();
  for (const b of bindings.property_bindings) {
    if (b.owner_id) {
      let set = nodePropertyIds.get(b.owner_id);
      if (!set) {
        set = new Set();
        nodePropertyIds.set(b.owner_id, set);
      }
      set.add(b.property_id);
    }
  }

  return { nodeIds, edgeIds, nodeKinds, edgeKinds, nodePropertyIds };
}

export function buildDiffSets(
  report: ReconcileReport | null,
  diffOverlay?: OntologyDiff | null,
): {
  addedIds: Set<string>;
  modifiedIds: Set<string>;
  removedIds: Set<string>;
} {
  const addedIds = new Set<string>();
  const modifiedIds = new Set<string>();
  const removedIds = new Set<string>();

  if (report) {
    for (const e of report.generated_ids) addedIds.add(e.id);
    for (const m of report.uncertain_matches) modifiedIds.add(m.original_id);
  }

  if (diffOverlay) {
    for (const n of diffOverlay.added_nodes) addedIds.add(n.id);
    for (const e of diffOverlay.added_edges) addedIds.add(e.id);
    for (const n of diffOverlay.modified_nodes) modifiedIds.add(n.node_id);
    for (const e of diffOverlay.modified_edges) modifiedIds.add(e.edge_id);
    for (const n of diffOverlay.removed_nodes) removedIds.add(n.id);
    for (const e of diffOverlay.removed_edges) removedIds.add(e.id);
  }

  return { addedIds, modifiedIds, removedIds };
}

// ---------------------------------------------------------------------------
// Node layer / diff status
// ---------------------------------------------------------------------------

export function nodeLayer(
  nodeDef: OntologyIR["node_types"][number],
  nodeGaps: QualityGap[],
  addedIds: Set<string>,
): NodeLayer {
  // Priority: problematic > suggested > asserted > inferred
  const hasHighGaps = nodeGaps.some((g) => g.severity === "high");
  if (hasHighGaps) return "problematic";
  if (addedIds.has(nodeDef.id)) return "suggested";
  if (nodeDef.source_table) return "asserted";
  return "inferred";
}

export function nodeDiffStatus(
  nodeId: string,
  addedIds: Set<string>,
  modifiedIds: Set<string>,
  removedIds?: Set<string>,
): DiffStatus | undefined {
  if (addedIds.has(nodeId)) return "added";
  if (removedIds?.has(nodeId)) return "removed";
  if (modifiedIds.has(nodeId)) return "modified";
  return undefined;
}

// ---------------------------------------------------------------------------
// Build React Flow nodes/edges from OntologyIR
// ---------------------------------------------------------------------------

const EMPTY_GAPS: QualityGap[] = [];

/** Build flow elements WITHOUT selection state -- selection is applied separately. */
export function buildFlowElements(
  ontology: OntologyIR,
  gapMap: Map<string, QualityGap[]>,
  highlightedBindings: ResolvedQueryBindings | null,
  reconcileReport: ReconcileReport | null,
  nodeGroups: Record<string, NodeGroup>,
  diffOverlay?: OntologyDiff | null,
): { nodes: Node[]; edges: Edge[] } {
  const hl = buildHighlightSets(highlightedBindings);
  const { addedIds, modifiedIds, removedIds } = buildDiffSets(reconcileReport, diffOverlay);

  // Build reverse lookup: nodeId -> groupId
  const nodeToGroup = new Map<string, string>();
  for (const [groupId, group] of Object.entries(nodeGroups)) {
    for (const nodeId of group.nodeIds) {
      nodeToGroup.set(nodeId, groupId);
    }
  }

  const nodes: Node[] = [];

  // Add group nodes first (they must come before child nodes in React Flow)
  for (const [groupId, group] of Object.entries(nodeGroups)) {
    const validNodeIds = group.nodeIds.filter((nid) =>
      ontology.node_types.some((n) => n.id === nid),
    );
    if (validNodeIds.length === 0) continue;

    nodes.push({
      id: groupId,
      type: "group",
      position: { x: 0, y: 0 },
      data: {
        groupId,
        name: group.name,
        nodeCount: validNodeIds.length,
        collapsed: group.collapsed,
        color: group.color,
      } satisfies GroupNodeData,
      style: group.collapsed
        ? { width: 180, height: 60 }
        : { width: 400, height: 300 },
    });
  }

  // Add schema nodes
  for (let i = 0; i < ontology.node_types.length; i++) {
    const n = ontology.node_types[i];
    const groupId = nodeToGroup.get(n.id);
    const group = groupId ? nodeGroups[groupId] : undefined;

    // If this node is in a collapsed group, hide it
    if (group?.collapsed) continue;

    const gaps = gapMap.get(n.id) ?? EMPTY_GAPS;
    const node: Node = {
      id: n.id,
      type: "schema",
      position: { x: i * 280, y: group ? 50 : 0 },
      data: {
        nodeDef: n,
        gaps,
        selected: false,
        highlighted: hl.nodeIds.has(n.id),
        highlightKind: hl.nodeKinds.get(n.id),
        highlightedPropertyIds: hl.nodePropertyIds.get(n.id),
        layer: nodeLayer(n, gaps, addedIds),
        diffStatus: nodeDiffStatus(n.id, addedIds, modifiedIds, removedIds),
        dimmed: false,
      } satisfies SchemaNodeData,
    };

    // Assign parent group
    if (groupId && !group?.collapsed) {
      node.parentId = groupId;
      node.extent = "parent";
    }

    nodes.push(node);
  }

  const edges: Edge[] = ontology.edge_types.map((e) => ({
    id: e.id,
    source: e.source_node_id,
    target: e.target_node_id,
    type: "schema",
    markerEnd: { type: "arrowclosed" as const, width: 16, height: 16 },
    data: {
      edgeDef: e,
      selected: false,
      highlighted: hl.edgeIds.has(e.id),
      highlightKind: hl.edgeKinds.get(e.id),
      diffStatus: nodeDiffStatus(e.id, addedIds, modifiedIds, removedIds),
    } satisfies SchemaEdgeData,
  }));

  return { nodes, edges };
}

// ---------------------------------------------------------------------------
// Export helpers -- capture canvas as PNG or SVG via html-to-image
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Auto-grouping for large ontologies (50+ nodes)
// ---------------------------------------------------------------------------

const GROUP_COLORS = [
  "#10b981", "#3b82f6", "#f59e0b", "#ef4444", "#8b5cf6",
  "#ec4899", "#06b6d4", "#84cc16", "#f97316", "#6366f1",
];

/**
 * Compute automatic node groups using connected-component analysis.
 * Groups nodes that are densely connected via edges.
 * Returns empty record if node count < threshold.
 */
export function computeAutoGroups(
  ontology: OntologyIR,
  threshold = 50,
): Record<string, NodeGroup> {
  if (ontology.node_types.length < threshold) return {};

  // Build adjacency + ID mappings
  const adj = new Map<string, Set<string>>(); // label → neighbor labels
  const idToLabel = new Map<string, string>();
  const labelToId = new Map<string, string>();
  for (const n of ontology.node_types) {
    adj.set(n.label, new Set());
    idToLabel.set(n.id, n.label);
    labelToId.set(n.label, n.id);
  }
  for (const e of ontology.edge_types) {
    const srcLabel = idToLabel.get(e.source_node_id);
    const tgtLabel = idToLabel.get(e.target_node_id);
    if (srcLabel && tgtLabel) {
      adj.get(srcLabel)?.add(tgtLabel);
      adj.get(tgtLabel)?.add(srcLabel);
    }
  }

  // BFS-based connected components (using labels internally)
  const visited = new Set<string>();
  const components: string[][] = []; // each component = array of labels

  for (const n of ontology.node_types) {
    if (visited.has(n.label)) continue;
    const component: string[] = [];
    const queue = [n.label];
    while (queue.length > 0) {
      const curr = queue.shift()!;
      if (visited.has(curr)) continue;
      visited.add(curr);
      component.push(curr);
      for (const neighbor of adj.get(curr) ?? []) {
        if (!visited.has(neighbor)) queue.push(neighbor);
      }
    }
    components.push(component);
  }

  // Helper: convert label array to node ID array (for group nodeIds)
  const labelsToIds = (labels: string[]): string[] =>
    labels.map((l) => labelToId.get(l)).filter((id): id is string => !!id);

  // If everything is one big component, split by degree
  if (components.length <= 1 && ontology.node_types.length >= threshold) {
    return splitByDegree(ontology, adj, labelToId);
  }

  // Convert components to groups (using node IDs, not labels)
  const groups: Record<string, NodeGroup> = {};
  components.forEach((comp, i) => {
    if (comp.length < 2) return;
    const groupId = `auto-group-${i}`;
    groups[groupId] = {
      name: `Cluster ${i + 1} (${comp.length} nodes)`,
      nodeIds: labelsToIds(comp),
      collapsed: true,
      color: GROUP_COLORS[i % GROUP_COLORS.length],
    };
  });

  return groups;
}

/** Split a single large connected component into sub-groups by node degree. */
function splitByDegree(
  ontology: OntologyIR,
  adj: Map<string, Set<string>>,
  labelToId: Map<string, string>,
): Record<string, NodeGroup> {
  const sorted = ontology.node_types
    .map((n) => ({ id: n.id, label: n.label, degree: adj.get(n.label)?.size ?? 0 }))
    .sort((a, b) => b.degree - a.degree);

  const targetGroupSize = Math.max(10, Math.min(15, Math.ceil(sorted.length / 8)));
  const groups: Record<string, NodeGroup> = {};
  let groupIdx = 0;

  for (let i = 0; i < sorted.length; i += targetGroupSize) {
    const chunk = sorted.slice(i, i + targetGroupSize);
    const groupId = `auto-group-${groupIdx}`;
    groups[groupId] = {
      name: `Domain ${groupIdx + 1} (${chunk.length} nodes)`,
      nodeIds: chunk.map((c) => c.id), // Use node IDs, not labels
      collapsed: true,
      color: GROUP_COLORS[groupIdx % GROUP_COLORS.length],
    };
    groupIdx++;
  }

  return groups;
}

function downloadBlob(dataUrl: string, filename: string) {
  const a = document.createElement("a");
  a.href = dataUrl;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
}

export async function exportCanvasImage(
  flowNodes: Node[],
  format: "png" | "svg",
  ontologyName: string,
) {
  const el = document.querySelector(".react-flow__viewport") as HTMLElement | null;
  if (!el || flowNodes.length === 0) {
    toast.error("Nothing to export");
    return;
  }
  const bounds = getNodesBounds(flowNodes);
  const padding = 50;
  const width = bounds.width + padding * 2;
  const height = bounds.height + padding * 2;
  const viewport = getViewportForBounds(bounds, width, height, 0.1, 2, padding);
  const options = {
    width,
    height,
    style: {
      width: String(width),
      height: String(height),
      transform: `translate(${viewport.x}px, ${viewport.y}px) scale(${viewport.zoom})`,
    },
  };
  const sanitized = ontologyName.replace(/[^a-zA-Z0-9_-]/g, "_") || "ontology";
  try {
    if (format === "png") {
      const dataUrl = await toPng(el, options);
      downloadBlob(dataUrl, `${sanitized}_ontology.png`);
    } else {
      const dataUrl = await toSvg(el, options);
      downloadBlob(dataUrl, `${sanitized}_ontology.svg`);
    }
    toast.success(`Exported as ${format.toUpperCase()}`);
  } catch {
    toast.error("Export failed");
  }
}
