import type { Node, Edge } from "@xyflow/react";
import type { UiConfig } from "@/types/api";
import type { WorkerResponse } from "./elk-layout.worker";
import { ELK_OPTIONS, buildElkOptions } from "./elk-options";

// ---------------------------------------------------------------------------
// ELK layout computation for ontology graph
//
// Offloads layout to a Web Worker to keep the main thread responsive.
// Falls back to main-thread computation if the worker cannot be created
// (e.g. during SSR or if the browser doesn't support module workers).
//
// Public API:
//   computeElkLayout(nodes, edges, config?) → Promise<LayoutResult>
//
// When `config` is provided, the main-thread fallback uses dynamic ELK
// options from the server. The worker uses static defaults (initialized
// at module load time — worker config updates require page reload).
// ---------------------------------------------------------------------------

let cachedWorkerTimeoutMs = 30_000;

export interface LayoutResult {
  nodes: Node[];
  edges: Edge[];
}

// -- Worker singleton -------------------------------------------------------

let worker: Worker | null = null;
let workerInitializing = false;

interface PendingRequest {
  resolve: (positions: Record<string, { x: number; y: number }>) => void;
  reject: (err: Error) => void;
  timer: ReturnType<typeof setTimeout>;
}

const pending = new Map<number, PendingRequest>();
let requestId = 0;

function getWorker(): Worker | null {
  if (worker) return worker;
  if (workerInitializing) return null;
  if (typeof window === "undefined") return null;

  workerInitializing = true;
  try {
    const w = new Worker(
      new URL("./elk-layout.worker.ts", import.meta.url),
    );

    w.onmessage = (event: MessageEvent<WorkerResponse>) => {
      const { id, positions, error } = event.data;
      const req = pending.get(id);
      if (!req) return;

      pending.delete(id);
      clearTimeout(req.timer);

      if (error) {
        req.reject(new Error(`ELK worker error: ${error}`));
      } else if (positions) {
        req.resolve(positions);
      } else {
        req.reject(new Error("ELK worker returned empty response"));
      }
    };

    w.onerror = (event) => {
      event.preventDefault();
      console.warn("[elk-layout] Worker error, falling back to main thread");
      worker = null;
      workerInitializing = false;
      for (const [, req] of pending) {
        clearTimeout(req.timer);
        req.reject(new Error("ELK worker encountered an error"));
      }
      pending.clear();
    };

    worker = w;
    return w;
  } catch (err) {
    console.warn("[elk-layout] Worker creation failed, falling back to main thread:", err);
    workerInitializing = false;
    return null;
  }
}

function layoutViaWorker(
  nodes: Node[],
  edges: Edge[],
): Promise<Record<string, { x: number; y: number }>> {
  const w = getWorker();
  if (!w) return Promise.reject(new Error("Worker unavailable"));

  const id = ++requestId;

  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      pending.delete(id);
      reject(new Error("ELK worker timed out"));
    }, cachedWorkerTimeoutMs);

    pending.set(id, { resolve, reject, timer });

    // Send serializable plain data only
    w.postMessage({
      id,
      nodes: nodes.map((node) => ({
        id: node.id,
        width: node.measured?.width ?? node.width ?? 220,
        height: node.measured?.height ?? node.height ?? 100,
      })),
      edges: edges.map((edge) => ({
        id: edge.id,
        source: edge.source,
        target: edge.target,
      })),
    });
  });
}

// -- Main-thread fallback ---------------------------------------------------

let elkInstance: import("elkjs/lib/elk-api").ELK | null = null;

async function layoutOnMainThread(
  nodes: Node[],
  edges: Edge[],
  config?: UiConfig,
): Promise<Record<string, { x: number; y: number }>> {
  if (!elkInstance) {
    const ELK = (await import("elkjs/lib/elk.bundled.js")).default;
    elkInstance = new ELK();
  }

  const elkGraph = {
    id: "root",
    layoutOptions: config ? buildElkOptions(config) : ELK_OPTIONS,
    children: nodes.map((node) => ({
      id: node.id,
      width: node.measured?.width ?? node.width ?? 220,
      height: node.measured?.height ?? node.height ?? 100,
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

  const layout = await elkInstance.layout(elkGraph);

  const positions: Record<string, { x: number; y: number }> = {};
  for (const child of layout.children ?? []) {
    positions[child.id] = { x: child.x ?? 0, y: child.y ?? 0 };
  }
  return positions;
}

// -- Public API -------------------------------------------------------------

/**
 * Update cached UiConfig for worker timeout.
 * Called once after fetching config from server.
 */
export function updateElkConfig(config: UiConfig) {
  cachedWorkerTimeoutMs = config.worker_timeout_ms;
}

/**
 * Compute ELK layout for React Flow nodes/edges.
 * Uses a Web Worker when available, falling back to main-thread computation.
 * When `config` is provided, the main-thread fallback uses dynamic layout options.
 */
export async function computeElkLayout(
  nodes: Node[],
  edges: Edge[],
  config?: UiConfig,
): Promise<LayoutResult> {
  let positions: Record<string, { x: number; y: number }>;

  try {
    positions = await layoutViaWorker(nodes, edges);
  } catch {
    // Worker unavailable, failed, or timed out — fall back to main thread
    positions = await layoutOnMainThread(nodes, edges, config);
  }

  const layoutNodes = nodes.map((node) => ({
    ...node,
    position: positions[node.id] ?? { x: 0, y: 0 },
  }));

  return { nodes: layoutNodes, edges };
}
