import type { Node, Edge } from "@xyflow/react";
import type { UiConfig } from "@/types/api";
import type { WorkerRequest, WorkerResponse } from "./elk-layout.worker";
import { ELK_OPTIONS, buildElkOptions } from "./elk-options";

// ---------------------------------------------------------------------------
// ELK layout computation for canvas surfaces
//
// Offloads every canvas's layout call to a shared Web Worker and falls
// back to main-thread computation only if the worker can't be created
// (SSR, older browsers without module-worker support).
//
// Public API:
//   computeElkLayout(nodes, edges, options?) → Promise<LayoutResult>
//
// `options.preset` picks the algorithm family:
//   - "layered" (default): ontology editor — hierarchical, ports enabled.
//                          Spacing / direction / edge-routing are tunable
//                          via `options.uiConfig` (server-supplied).
//   - "stress": explore surface — force-like placement, no ports (ports
//               would fight the spring model). Static options for now.
//   - "mrtree": top-down tree layout (root-to-leaves). Best for taxonomy
//               or pure hierarchies where every node has a single parent.
//   - "radial": radial-tree placement around a focused root. Good for
//               showcasing concentric structure (centre-of-graph + hops).
//   - "force":  spring-electric force-directed. The general-graph default
//               when the structure isn't hierarchical or tree-shaped.
//
// All presets share one worker. That avoids spawning the 1 MB ELK
// bundle per algorithm and lets us scale new presets by registering
// here rather than duplicating the pipeline.
// ---------------------------------------------------------------------------

let cachedWorkerTimeoutMs = 30_000;

export interface LayoutResult {
  nodes: Node[];
  edges: Edge[];
}

export type ElkLayoutPreset =
  | "layered"
  | "stress"
  | "mrtree"
  | "radial"
  | "force";

/**
 * Every supported preset paired with its translation key. Picker UI
 * iterates this constant so adding a sixth algorithm is one entry,
 * one i18n key, and one branch in `resolvePreset`. Order is the
 * order users see in the toolbar dropdown.
 */
export const ELK_LAYOUT_PRESETS: ReadonlyArray<{
  id: ElkLayoutPreset;
  labelKey: string;
}> = [
  { id: "layered", labelKey: "layered" },
  { id: "mrtree", labelKey: "mrtree" },
  { id: "radial", labelKey: "radial" },
  { id: "force", labelKey: "force" },
  { id: "stress", labelKey: "stress" },
];

export interface ComputeElkOptions {
  /** Algorithm preset. Defaults to "layered". */
  preset?: ElkLayoutPreset;
  /**
   * Server-supplied layout tunables. Only consulted for the "layered"
   * preset (direction / node + layer spacing / edge routing). Ignored
   * for other presets that have their own spacing rationale.
   */
  uiConfig?: UiConfig;
}

// -- Preset resolution -------------------------------------------------------

/** Stress majorization — explore canvas default. Minimises edge-length
 * distortion and looks good for general (non-hierarchical) graphs
 * without pulling in d3-force. No ports — they fight the spring
 * model.
 */
const STRESS_OPTIONS: Readonly<Record<string, string>> = Object.freeze({
  "elk.algorithm": "stress",
  "elk.spacing.nodeNode": "80",
  "elk.stress.epsilon": "0.0001",
  "elk.stress.iterationLimit": "400",
});

/** Mr-Tree — strict top-down tree placement. Cleanest for ontologies
 * whose backbone is a single inheritance / containment tree.
 */
const MRTREE_OPTIONS: Readonly<Record<string, string>> = Object.freeze({
  "elk.algorithm": "mrtree",
  "elk.direction": "DOWN",
  "elk.spacing.nodeNode": "60",
  "elk.mrtree.spacing.levelLevel": "80",
});

/** Radial — concentric layout around a focused root. Good for graphs
 * with a clear centre + hop-distance interpretation (a single
 * NodeType the user is exploring outward from).
 */
const RADIAL_OPTIONS: Readonly<Record<string, string>> = Object.freeze({
  "elk.algorithm": "radial",
  "elk.spacing.nodeNode": "80",
  "elk.radial.radius": "240",
  "elk.radial.optimizationCriteria": "ANNULUS_WEDGE_BY_NODES",
});

/** Force — spring-electric force-directed. The general-graph default
 * when neither hierarchy nor tree shape applies.
 */
const FORCE_OPTIONS: Readonly<Record<string, string>> = Object.freeze({
  "elk.algorithm": "force",
  "elk.spacing.nodeNode": "100",
  "elk.force.iterations": "300",
  "elk.force.repulsivePower": "1",
});

interface ResolvedPreset {
  layoutOptions: Record<string, string>;
  withPorts: boolean;
}

function resolvePreset(options: ComputeElkOptions | undefined): ResolvedPreset {
  const preset = options?.preset ?? "layered";
  switch (preset) {
    case "stress":
      return { layoutOptions: { ...STRESS_OPTIONS }, withPorts: false };
    case "mrtree":
      return { layoutOptions: { ...MRTREE_OPTIONS }, withPorts: false };
    case "radial":
      return { layoutOptions: { ...RADIAL_OPTIONS }, withPorts: false };
    case "force":
      return { layoutOptions: { ...FORCE_OPTIONS }, withPorts: false };
    default:
      return {
        layoutOptions: options?.uiConfig ? buildElkOptions(options.uiConfig) : { ...ELK_OPTIONS },
        withPorts: true,
      };
  }
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
  resolved: ResolvedPreset,
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

    const request: WorkerRequest = {
      id,
      layoutOptions: resolved.layoutOptions,
      withPorts: resolved.withPorts,
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
    };
    w.postMessage(request);
  });
}

// -- Main-thread fallback ---------------------------------------------------

let elkInstance: import("elkjs/lib/elk-api").ELK | null = null;

async function layoutOnMainThread(
  nodes: Node[],
  edges: Edge[],
  resolved: ResolvedPreset,
): Promise<Record<string, { x: number; y: number }>> {
  if (!elkInstance) {
    const ELK = (await import("elkjs/lib/elk.bundled.js")).default;
    elkInstance = new ELK();
  }

  const elkGraph = {
    id: "root",
    layoutOptions: resolved.layoutOptions,
    children: nodes.map((node) => {
      const base = {
        id: node.id,
        width: node.measured?.width ?? node.width ?? 220,
        height: node.measured?.height ?? node.height ?? 100,
      };
      if (!resolved.withPorts) return base;
      return {
        ...base,
        ports: [
          { id: `${node.id}:top`, properties: { "port.side": "NORTH" } },
          { id: `${node.id}:bottom`, properties: { "port.side": "SOUTH" } },
          { id: `${node.id}:left`, properties: { "port.side": "WEST" } },
          { id: `${node.id}:right`, properties: { "port.side": "EAST" } },
        ],
      };
    }),
    edges: edges.map((edge) =>
      resolved.withPorts
        ? {
            id: edge.id,
            sources: [`${edge.source}:right`],
            targets: [`${edge.target}:left`],
          }
        : { id: edge.id, sources: [edge.source], targets: [edge.target] },
    ),
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
 * `options.preset` selects the algorithm family (default "layered");
 * `options.uiConfig` applies server-supplied spacing for the "layered"
 * preset only.
 */
export async function computeElkLayout(
  nodes: Node[],
  edges: Edge[],
  options?: ComputeElkOptions,
): Promise<LayoutResult> {
  const resolved = resolvePreset(options);
  let positions: Record<string, { x: number; y: number }>;

  try {
    positions = await layoutViaWorker(nodes, edges, resolved);
  } catch {
    // Worker unavailable, failed, or timed out — fall back to main thread
    positions = await layoutOnMainThread(nodes, edges, resolved);
  }

  const layoutNodes = nodes.map((node) => ({
    ...node,
    position: positions[node.id] ?? { x: 0, y: 0 },
  }));

  return { nodes: layoutNodes, edges };
}
