import ELK from "elkjs/lib/elk.bundled.js";

// ---------------------------------------------------------------------------
// ELK worker — layout computation off the main thread
// ---------------------------------------------------------------------------
//
// The worker is preset-agnostic: every per-canvas concern (algorithm,
// spacing, edge routing, whether to stitch port ids onto each node) is
// decided by the caller and shipped in the request. That keeps the
// worker a single piece of infrastructure shared by every canvas, and
// the presets colocated with `computeElkLayout` in `elk-layout.ts`.

const elk = new ELK({ workerUrl: "" });

export interface WorkerRequest {
  id: number;
  layoutOptions: Record<string, string>;
  /**
   * When true, add N/S/E/W ports to every node and route edges from the
   * source's right port to the target's left port. Required for the
   * hierarchical `layered` algorithm (ontology canvas); counter-productive
   * for force-like algorithms such as `stress` (explore canvas) where
   * ports can fight the spring model.
   */
  withPorts: boolean;
  nodes: Array<{ id: string; width: number; height: number }>;
  edges: Array<{ id: string; source: string; target: string }>;
}

export interface WorkerResponse {
  id: number;
  positions?: Record<string, { x: number; y: number }>;
  error?: string;
}

self.onmessage = async (event: MessageEvent<WorkerRequest>) => {
  const { id, layoutOptions, withPorts, nodes, edges } = event.data;

  try {
    const elkGraph = {
      id: "root",
      layoutOptions,
      children: nodes.map((node) =>
        withPorts
          ? {
              id: node.id,
              width: node.width,
              height: node.height,
              ports: [
                { id: `${node.id}:top`, properties: { "port.side": "NORTH" } },
                { id: `${node.id}:bottom`, properties: { "port.side": "SOUTH" } },
                { id: `${node.id}:left`, properties: { "port.side": "WEST" } },
                { id: `${node.id}:right`, properties: { "port.side": "EAST" } },
              ],
            }
          : { id: node.id, width: node.width, height: node.height },
      ),
      edges: edges.map((edge) =>
        withPorts
          ? {
              id: edge.id,
              sources: [`${edge.source}:right`],
              targets: [`${edge.target}:left`],
            }
          : { id: edge.id, sources: [edge.source], targets: [edge.target] },
      ),
    };

    const layout = await elk.layout(elkGraph);

    const positions: Record<string, { x: number; y: number }> = {};
    for (const child of layout.children ?? []) {
      positions[child.id] = { x: child.x ?? 0, y: child.y ?? 0 };
    }

    const response: WorkerResponse = { id, positions };
    self.postMessage(response);
  } catch (err) {
    const response: WorkerResponse = {
      id,
      error: err instanceof Error ? err.message : String(err),
    };
    self.postMessage(response);
  }
};
