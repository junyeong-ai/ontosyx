import type { OntologyIR } from "@/types/api";

// ---------------------------------------------------------------------------
// Neighborhood computation — returns all node IDs within N hops of a node
// ---------------------------------------------------------------------------

/**
 * Compute the set of node IDs reachable within `depth` hops from `nodeId`,
 * using the ontology's edge_types as the adjacency list (undirected).
 */
export function getNeighborhood(
  ontology: OntologyIR,
  nodeId: string,
  depth: number,
): Set<string> {
  // Build adjacency list (undirected — both directions)
  const adj = new Map<string, Set<string>>();
  for (const edge of ontology.edge_types) {
    if (!adj.has(edge.source_node_id)) adj.set(edge.source_node_id, new Set());
    if (!adj.has(edge.target_node_id)) adj.set(edge.target_node_id, new Set());
    adj.get(edge.source_node_id)!.add(edge.target_node_id);
    adj.get(edge.target_node_id)!.add(edge.source_node_id);
  }

  const visited = new Set<string>([nodeId]);
  let frontier = new Set<string>([nodeId]);

  for (let hop = 0; hop < depth; hop++) {
    const nextFrontier = new Set<string>();
    for (const current of frontier) {
      const neighbors = adj.get(current);
      if (!neighbors) continue;
      for (const neighbor of neighbors) {
        if (!visited.has(neighbor)) {
          visited.add(neighbor);
          nextFrontier.add(neighbor);
        }
      }
    }
    if (nextFrontier.size === 0) break;
    frontier = nextFrontier;
  }

  return visited;
}

/**
 * Get the set of edge IDs that connect nodes within the neighborhood set.
 */
export function getNeighborhoodEdges(
  ontology: OntologyIR,
  neighborhoodNodeIds: Set<string>,
): Set<string> {
  const edgeIds = new Set<string>();
  for (const edge of ontology.edge_types) {
    if (neighborhoodNodeIds.has(edge.source_node_id) && neighborhoodNodeIds.has(edge.target_node_id)) {
      edgeIds.add(edge.id);
    }
  }
  return edgeIds;
}
