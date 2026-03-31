// ---------------------------------------------------------------------------
// Internal node/link types — enriched with rendering data.
// Fields are attached directly to the objects passed to react-force-graph-2d
// which uses { [others: string]: any } for extensibility.
// ---------------------------------------------------------------------------

export interface GraphNodeData {
  id: string;
  label: string;
  type?: string;
  properties: Record<string, unknown>;
  __color: string;
  __size: number;
}

export interface GraphLinkData {
  id: string;
  source: string;
  target: string;
  label?: string;
  properties: Record<string, unknown>;
}

/** react-force-graph node object at runtime (after simulation injects x/y) */
export type FGNode = GraphNodeData & { x?: number; y?: number };
/** react-force-graph link object at runtime */
export type FGLink = GraphLinkData;
