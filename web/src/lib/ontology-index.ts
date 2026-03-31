import type { OntologyIR, NodeTypeDef, EdgeTypeDef } from "@/types/api";


/**
 * O(1) lookup index over OntologyIR node/edge arrays.
 *
 * Trades a single O(N) build pass for O(1) per lookup, useful when the graph
 * canvas, inspector, and explorer all need to resolve entities by ID on every
 * render cycle.  Rebuild only when the ontology reference changes.
 */
export interface OntologyIndex {
  nodeById: Map<string, NodeTypeDef>;
  edgeById: Map<string, EdgeTypeDef>;
  /** All edge IDs that touch a given node ID (source or target). */
  edgesByNodeId: Map<string, EdgeTypeDef[]>;
}

export function buildOntologyIndex(ontology: OntologyIR): OntologyIndex {
  const nodeById = new Map<string, NodeTypeDef>();
  for (const n of ontology.node_types) {
    nodeById.set(n.id, n);
  }

  const edgeById = new Map<string, EdgeTypeDef>();
  const edgesByNodeId = new Map<string, EdgeTypeDef[]>();
  for (const e of ontology.edge_types) {
    edgeById.set(e.id, e);

    const srcList = edgesByNodeId.get(e.source_node_id) ?? [];
    srcList.push(e);
    edgesByNodeId.set(e.source_node_id, srcList);

    if (e.target_node_id !== e.source_node_id) {
      const tgtList = edgesByNodeId.get(e.target_node_id) ?? [];
      tgtList.push(e);
      edgesByNodeId.set(e.target_node_id, tgtList);
    }
  }

  return { nodeById, edgeById, edgesByNodeId };
}
