import type { QueryResult, NodeVizConfig, EdgeVizConfig } from "@/types/api";
import type { GraphNodeData, GraphLinkData } from "./graph-types";
import { resolveColorMap, assignNodeColor, assignNodeSize } from "./graph-utils";

// ---------------------------------------------------------------------------
// Data extraction — transforms QueryResult rows into graph nodes & links
// ---------------------------------------------------------------------------

export interface ExtractedGraph {
  nodes: GraphNodeData[];
  links: GraphLinkData[];
  totalNodes: number;
  totalLinks: number;
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
      const id = String(row[idCol] ?? "");
      if (!id) continue;

      const properties: Record<string, unknown> = {};
      for (const c of cols) {
        if (row[c] != null) properties[c] = row[c];
      }

      const nodeType = typeCol ? String(row[typeCol] ?? "") : undefined;
      const node: GraphNodeData = {
        id,
        label: String(row[labelCol] ?? id),
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
        const src = String(row[col1] ?? "");
        const tgt = String(row[col2] ?? "");
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
          label: edgeLabelCol ? String(row[edgeLabelCol] ?? "") : undefined,
          properties: edgeProps,
        });
      }
    }
  }

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
