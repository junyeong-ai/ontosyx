import type { QueryResult, NodeVizConfig, EdgeVizConfig } from "@/types/api";
import type { GraphNodeData, GraphLinkData } from "./graph-types";
import { resolveColorMap, assignNodeColor, assignNodeSize } from "./graph-utils";

// ---------------------------------------------------------------------------
// Value resolution — safely convert cell values to display strings
// ---------------------------------------------------------------------------

/** Safely convert a cell value to a display string.
 *  Handles: string, number, PropertyValue {type,value}, node objects, null. */
function resolveDisplayValue(value: unknown): string {
  if (value == null) return "";
  if (typeof value === "string") return value;
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  if (typeof value === "object") {
    const obj = value as Record<string, unknown>;
    // PropertyValue wrapper: {type: "string", value: "..."}
    if ("type" in obj && "value" in obj) return resolveDisplayValue(obj.value);
    if ("type" in obj && obj.type === "null") return "";
    // Node object: prefer name > label > title > id
    for (const key of ["name", "label", "title", "id"]) {
      if (key in obj && obj[key] != null) return resolveDisplayValue(obj[key]);
    }
    // Fallback: truncated JSON
    try { return JSON.stringify(value).slice(0, 80); } catch { return "(object)"; }
  }
  return String(value);
}

// ---------------------------------------------------------------------------
// Data extraction — transforms QueryResult rows into graph nodes & links
// ---------------------------------------------------------------------------

export interface ExtractedGraph {
  nodes: GraphNodeData[];
  links: GraphLinkData[];
  totalNodes: number;
  totalLinks: number;
}

/** Compute structural properties for nodes that lack their own. */
function enrichNodes(
  nodeMap: Map<string, GraphNodeData>,
  links: GraphLinkData[],
): void {
  if (links.length === 0) return;
  const needsEnrichment = [...nodeMap.values()].some(
    (n) => Object.keys(n.properties).length === 0,
  );
  if (!needsEnrichment) return;

  const degrees = new Map<string, number>();
  const relTypes = new Map<string, Set<string>>();
  const neighborLabels = new Map<string, Set<string>>();

  for (const link of links) {
    const src = link.source;
    const tgt = link.target;

    degrees.set(src, (degrees.get(src) ?? 0) + 1);
    degrees.set(tgt, (degrees.get(tgt) ?? 0) + 1);

    if (link.label) {
      if (!relTypes.has(src)) relTypes.set(src, new Set());
      if (!relTypes.has(tgt)) relTypes.set(tgt, new Set());
      relTypes.get(src)!.add(link.label);
      relTypes.get(tgt)!.add(link.label);
    }

    if (src !== tgt) {
      const srcNode = nodeMap.get(src);
      const tgtNode = nodeMap.get(tgt);
      if (!neighborLabels.has(src)) neighborLabels.set(src, new Set());
      if (!neighborLabels.has(tgt)) neighborLabels.set(tgt, new Set());
      neighborLabels.get(src)!.add(tgtNode?.label ?? tgt);
      neighborLabels.get(tgt)!.add(srcNode?.label ?? src);
    }
  }

  const MAX_NEIGHBORS = 5;
  for (const [id, node] of nodeMap) {
    if (Object.keys(node.properties).length > 0) continue;
    const deg = degrees.get(id) ?? 0;
    const types = relTypes.get(id);
    const nbrs = neighborLabels.get(id);

    node.properties = {
      connections: deg,
      ...(types?.size
        ? { relationship_types: [...types].join(", ") }
        : {}),
      ...(nbrs?.size
        ? {
            neighbors:
              nbrs.size <= MAX_NEIGHBORS
                ? [...nbrs].join(", ")
                : [...nbrs].slice(0, MAX_NEIGHBORS).join(", ") +
                  ` (+${nbrs.size - MAX_NEIGHBORS})`,
          }
        : {}),
    };
  }
}

export function extractGraphData(
  data: QueryResult,
  nodeConfig: NodeVizConfig | undefined,
  edgeConfig: EdgeVizConfig | undefined,
  maxNodes: number,
): ExtractedGraph {
  const colorField = nodeConfig?.color_field;
  const colorMap = resolveColorMap(nodeConfig);
  const sizeField = nodeConfig?.size_field;
  const labelField = nodeConfig?.label_field ?? "label";
  const typeColorIndex = new Map<string, string>();

  const cols = data.columns;
  const lowerCols = cols.map((c) => c.toLowerCase());

  const hasId = lowerCols.includes("id");
  const hasSource = lowerCols.includes("source") || lowerCols.includes("source_id") || lowerCols.includes("from");
  const hasTarget = lowerCols.includes("target") || lowerCols.includes("target_id") || lowerCols.includes("to");

  const nodeMap = new Map<string, GraphNodeData>();
  const links: GraphLinkData[] = [];

  if (hasSource && hasTarget) {
    // Edge-centric result: each row represents an edge
    const sourceCol = cols.find((c) => ["source", "source_id", "from"].includes(c.toLowerCase())) ?? "source";
    const targetCol = cols.find((c) => ["target", "target_id", "to"].includes(c.toLowerCase())) ?? "target";
    const edgeLabelCol = edgeConfig?.label_field
      ?? cols.find((c) => ["relationship", "type", "edge_type", "rel_type", "label"].includes(c.toLowerCase()));

    for (const row of data.rows) {
      const src = String(row[sourceCol] ?? "");
      const tgt = String(row[targetCol] ?? "");
      if (!src || !tgt) continue;

      // Ensure source node
      if (!nodeMap.has(src)) {
        const node: GraphNodeData = {
          id: src,
          label: src,
          type: undefined,
          properties: {},
          __color: "",
          __size: 4,
        };
        node.__color = assignNodeColor(node, colorField, colorMap, typeColorIndex);
        node.__size = assignNodeSize(node, sizeField);
        nodeMap.set(src, node);
      }

      // Ensure target node
      if (!nodeMap.has(tgt)) {
        const node: GraphNodeData = {
          id: tgt,
          label: tgt,
          type: undefined,
          properties: {},
          __color: "",
          __size: 4,
        };
        node.__color = assignNodeColor(node, colorField, colorMap, typeColorIndex);
        node.__size = assignNodeSize(node, sizeField);
        nodeMap.set(tgt, node);
      }

      const edgeProps: Record<string, unknown> = {};
      for (const c of cols) {
        if (c === sourceCol || c === targetCol) continue;
        if (row[c] != null) edgeProps[c] = row[c];
      }

      links.push({
        id: `${src}->${tgt}:${links.length}`,
        source: src,
        target: tgt,
        label: edgeLabelCol ? String(row[edgeLabelCol] ?? "") : undefined,
        properties: edgeProps,
      });
    }
  } else if (hasId) {
    // Node-centric result: each row is a node
    const idCol = cols.find((c) => c.toLowerCase() === "id") ?? "id";
    const labelCol = cols.find((c) => c.toLowerCase() === labelField.toLowerCase())
      ?? cols.find((c) => ["label", "name", "title"].includes(c.toLowerCase()))
      ?? idCol;
    const typeCol = cols.find((c) => ["type", "node_type", "label_type", "category"].includes(c.toLowerCase()));

    for (const row of data.rows) {
      const id = resolveDisplayValue(row[idCol]);
      if (!id) continue;

      const properties: Record<string, unknown> = {};
      for (const c of cols) {
        if (row[c] != null) properties[c] = row[c];
      }

      const nodeType = typeCol ? resolveDisplayValue(row[typeCol]) : undefined;
      const node: GraphNodeData = {
        id,
        label: resolveDisplayValue(row[labelCol]) || id,
        type: nodeType,
        properties,
        __color: "",
        __size: 4,
      };
      node.__color = assignNodeColor(node, colorField, colorMap, typeColorIndex);
      node.__size = assignNodeSize(node, sizeField);
      nodeMap.set(id, node);
    }
  } else {
    // Fallback: treat first two columns as source/target
    if (cols.length >= 2) {
      const [col1, col2] = cols;
      const edgeLabelCol = edgeConfig?.label_field ?? cols[2];

      for (const row of data.rows) {
        const src = resolveDisplayValue(row[col1]);
        const tgt = resolveDisplayValue(row[col2]);
        if (!src || !tgt) continue;

        if (!nodeMap.has(src)) {
          const node: GraphNodeData = {
            id: src,
            label: src,
            properties: {},
            __color: "",
            __size: 4,
          };
          node.__color = assignNodeColor(node, colorField, colorMap, typeColorIndex);
          nodeMap.set(src, node);
        }
        if (!nodeMap.has(tgt)) {
          const node: GraphNodeData = {
            id: tgt,
            label: tgt,
            properties: {},
            __color: "",
            __size: 4,
          };
          node.__color = assignNodeColor(node, colorField, colorMap, typeColorIndex);
          nodeMap.set(tgt, node);
        }

        const edgeProps: Record<string, unknown> = {};
        for (const c of cols.slice(2)) {
          if (row[c] != null) edgeProps[c] = row[c];
        }

        links.push({
          id: `${src}->${tgt}:${links.length}`,
          source: src,
          target: tgt,
          label: edgeLabelCol ? resolveDisplayValue(row[edgeLabelCol]) : undefined,
          properties: edgeProps,
        });
      }
    }
  }

  enrichNodes(nodeMap, links);

  const allNodes = Array.from(nodeMap.values());
  const totalNodes = allNodes.length;
  const totalLinks = links.length;

  // Apply max_nodes limit
  const limitedNodes = allNodes.slice(0, maxNodes);
  const limitedNodeIds = new Set(limitedNodes.map((n) => n.id));
  const limitedLinks = links.filter(
    (l) => limitedNodeIds.has(l.source) && limitedNodeIds.has(l.target),
  );

  return {
    nodes: limitedNodes,
    links: limitedLinks,
    totalNodes,
    totalLinks,
  };
}
