import ELK from "elkjs/lib/elk.bundled.js";
import { ELK_OPTIONS } from "./elk-options";

const elk = new ELK({ workerUrl: "" });

export interface WorkerRequest {
  id: number;
  nodes: Array<{ id: string; width: number; height: number }>;
  edges: Array<{ id: string; source: string; target: string }>;
}

export interface WorkerResponse {
  id: number;
  positions?: Record<string, { x: number; y: number }>;
  error?: string;
}

self.onmessage = async (event: MessageEvent<WorkerRequest>) => {
  const { id, nodes, edges } = event.data;

  try {
    const elkGraph = {
      id: "root",
      layoutOptions: ELK_OPTIONS,
      children: nodes.map((node) => ({
        id: node.id,
        width: node.width,
        height: node.height,
        ports: [
          { id: `${node.id}:top`, properties: { "port.side": "NORTH" } },
          { id: `${node.id}:bottom`, properties: { "port.side": "SOUTH" } },
          { id: `${node.id}:left`, properties: { "port.side": "WEST" } },
          { id: `${node.id}:right`, properties: { "port.side": "EAST" } },
        ],
      })),
      edges: edges.map((edge) => ({
        id: edge.id,
        sources: [`${edge.source}:right`],
        targets: [`${edge.target}:left`],
      })),
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
