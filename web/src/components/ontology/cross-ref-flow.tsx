"use client";

import { useMemo, useState } from "react";
import { useTranslations } from "next-intl";
import { useQuery } from "@tanstack/react-query";
import {
  ReactFlow,
  ReactFlowProvider,
  MarkerType,
  Handle,
  Position,
  type Node,
  type Edge,
  type NodeProps,
  type EdgeMouseHandler,
} from "@xyflow/react";

import { request } from "@/lib/api/client";
import { Spinner } from "@/components/ui/spinner";
import { EmptyState } from "@/components/ui/empty-state";
import { useIsDarkMode } from "@/hooks/use-dark-mode";

// ---------------------------------------------------------------------------
// Wire shapes — mirror the Rust `CrossRefEdge` + `Axis` exactly so
// a compile of the backend contract leaves this component green.
// ---------------------------------------------------------------------------

export type AxisKey =
  | "topology"
  | "vocabulary"
  | "registry"
  | "strategy"
  | "vol"
  | "governance";

export interface CrossRefEdge {
  source_axis: AxisKey;
  source_kind: string;
  source_id: string;
  edge_kind: string;
  target_axis: AxisKey;
  target_kind: string;
  target_id: string;
}

async function fetchCrossRefs(id: string): Promise<CrossRefEdge[]> {
  return request<CrossRefEdge[]>(
    `/ontologies/${encodeURIComponent(id)}/cross-refs`,
  );
}

// ---------------------------------------------------------------------------
// Layout — six axis nodes at hex vertices. The hex has a fixed
// radius so the canvas size below can be static; a responsive
// layout can land later if the view moves into a resizable panel.
// ---------------------------------------------------------------------------

const AXES: AxisKey[] = [
  "topology",
  "vocabulary",
  "registry",
  "strategy",
  "vol",
  "governance",
];

const HEX_RADIUS = 160;
const CENTER_X = 260;
const CENTER_Y = 200;

/**
 * Return the `(x, y)` position for the axis at `index` in a six-
 * vertex regular hex. Starting at the top (−π/2) and rotating
 * clockwise so the visual order reads Topology (12 o'clock) →
 * Vocabulary → Registry → Strategy → VOL → Governance.
 */
function axisPosition(index: number): { x: number; y: number } {
  const angle = -Math.PI / 2 + (index * Math.PI) / 3;
  return {
    x: CENTER_X + HEX_RADIUS * Math.cos(angle),
    y: CENTER_Y + HEX_RADIUS * Math.sin(angle),
  };
}

// Fixed axis palette — matches the Complete Map card order
// so an operator's eye learns "Topology is always blue".
const AXIS_COLOR: Record<AxisKey, { ring: string; fill: string; stroke: string }> =
  {
    topology: { ring: "#60a5fa", fill: "#dbeafe", stroke: "#2563eb" },
    vocabulary: { ring: "#a78bfa", fill: "#ede9fe", stroke: "#7c3aed" },
    registry: { ring: "#34d399", fill: "#d1fae5", stroke: "#059669" },
    strategy: { ring: "#fbbf24", fill: "#fef3c7", stroke: "#d97706" },
    vol: { ring: "#fb7185", fill: "#ffe4e6", stroke: "#e11d48" },
    governance: { ring: "#94a3b8", fill: "#e2e8f0", stroke: "#475569" },
  };

// ---------------------------------------------------------------------------
// Aggregation — flat edges → `(src, tgt) → { count, edges[] }`.
// ---------------------------------------------------------------------------

export interface AggregatedBucket {
  source: AxisKey;
  target: AxisKey;
  count: number;
  edges: CrossRefEdge[];
}

export function aggregateEdges(edges: CrossRefEdge[]): AggregatedBucket[] {
  const map = new Map<string, AggregatedBucket>();
  for (const e of edges) {
    const key = `${e.source_axis}→${e.target_axis}`;
    const existing = map.get(key);
    if (existing) {
      existing.count += 1;
      existing.edges.push(e);
    } else {
      map.set(key, {
        source: e.source_axis,
        target: e.target_axis,
        count: 1,
        edges: [e],
      });
    }
  }
  return Array.from(map.values());
}

// ---------------------------------------------------------------------------
// Axis node — the React Flow custom node. Simple rounded pill with
// the axis name + total "outgoing" count as a chip.
// ---------------------------------------------------------------------------

interface AxisNodeData {
  label: string;
  outgoing: number;
  incoming: number;
  axis: AxisKey;
  [key: string]: unknown;
}

function AxisNode({ data }: NodeProps<Node<AxisNodeData>>) {
  const colors = AXIS_COLOR[data.axis];
  return (
    <div
      className="flex min-w-[110px] flex-col items-center rounded-lg border-2 px-3 py-2 shadow-sm"
      style={{
        background: colors.fill,
        borderColor: colors.ring,
        color: colors.stroke,
      }}
    >
      <span className="text-xs font-semibold">{data.label}</span>
      <span className="mt-0.5 text-2xs text-foreground">
        {data.outgoing + data.incoming} refs
      </span>
      {/* Hidden handles — React Flow needs at least one source +
          one target to route edges; we keep both on every side so
          the curve router picks the shortest path. */}
      <Handle type="source" position={Position.Top} style={{ opacity: 0 }} />
      <Handle type="source" position={Position.Right} style={{ opacity: 0 }} />
      <Handle type="source" position={Position.Bottom} style={{ opacity: 0 }} />
      <Handle type="source" position={Position.Left} style={{ opacity: 0 }} />
      <Handle type="target" position={Position.Top} style={{ opacity: 0 }} />
      <Handle type="target" position={Position.Right} style={{ opacity: 0 }} />
      <Handle type="target" position={Position.Bottom} style={{ opacity: 0 }} />
      <Handle type="target" position={Position.Left} style={{ opacity: 0 }} />
    </div>
  );
}

const nodeTypes = { axis: AxisNode };

// ---------------------------------------------------------------------------
// Exported component
// ---------------------------------------------------------------------------

export function CrossRefFlow({ ontologyId }: { ontologyId: string }) {
  const t = useTranslations("ontology.map.crossRef");
  const isDark = useIsDarkMode();
  const [selected, setSelected] = useState<AggregatedBucket | null>(null);

  const { data, isLoading, error } = useQuery({
    queryKey: ["ontology-cross-refs", ontologyId],
    queryFn: () => fetchCrossRefs(ontologyId),
  });

  const { nodes, edges, buckets } = useMemo(() => {
    const list = data ?? [];
    const buckets = aggregateEdges(list);

    // Per-axis totals, used to size each axis node's "refs" chip.
    const outgoing: Record<AxisKey, number> = {
      topology: 0,
      vocabulary: 0,
      registry: 0,
      strategy: 0,
      vol: 0,
      governance: 0,
    };
    const incoming: Record<AxisKey, number> = {
      topology: 0,
      vocabulary: 0,
      registry: 0,
      strategy: 0,
      vol: 0,
      governance: 0,
    };
    for (const b of buckets) {
      outgoing[b.source] += b.count;
      if (b.source !== b.target) incoming[b.target] += b.count;
    }

    const nodes: Node<AxisNodeData>[] = AXES.map((axis, idx) => {
      const pos = axisPosition(idx);
      return {
        id: axis,
        type: "axis",
        position: pos,
        draggable: false,
        data: {
          label: t(`axisLabel.${axis}`),
          axis,
          outgoing: outgoing[axis],
          incoming: incoming[axis],
        },
      };
    });

    // Edge thickness scales logarithmically on count so a 200-edge
    // bucket doesn't dwarf a 5-edge bucket but still visibly beats
    // it. Self-loops (source === target) get a small offset curve
    // via `type: "default"`.
    const maxCount = buckets.reduce((m, b) => Math.max(m, b.count), 1);
    const edges: Edge[] = buckets.map((b) => ({
      id: `${b.source}-${b.target}`,
      source: b.source,
      target: b.target,
      type: b.source === b.target ? "default" : "smoothstep",
      style: {
        stroke: AXIS_COLOR[b.source].stroke,
        strokeWidth: 1 + (Math.log(1 + b.count) / Math.log(1 + maxCount)) * 4,
        cursor: "pointer",
      },
      label: String(b.count),
      labelStyle: { fill: AXIS_COLOR[b.source].stroke, fontSize: 10 },
      labelBgStyle: {
        fill: isDark ? "#18181b" : "#ffffff",
        fillOpacity: 0.85,
      },
      markerEnd: {
        type: MarkerType.ArrowClosed,
        color: AXIS_COLOR[b.source].stroke,
        width: 16,
        height: 16,
      },
    }));

    return { nodes, edges, buckets };
  }, [data, t, isDark]);

  const onEdgeClick: EdgeMouseHandler = (_e, edge) => {
    const bucket = buckets.find((b) => `${b.source}-${b.target}` === edge.id);
    if (bucket) setSelected(bucket);
  };

  return (
    <section className="mt-6 rounded-lg border border-divider bg-surface-base">
      <header className="border-b border-divider px-5 py-3">
        <h2 className="text-sm font-semibold text-foreground-strong">
          {t("title")}
        </h2>
        <p className="mt-0.5 text-[11px] text-muted-foreground">
          {t("subtitle")}
        </p>
      </header>

      {isLoading && (
        <div className="flex h-[420px] items-center justify-center">
          <Spinner />
        </div>
      )}
      {error && (
        <p className="px-5 py-8 text-center text-xs text-danger-foreground dark:text-danger-foreground">
          {t("loadError", {
            message: error instanceof Error ? error.message : t("unknownError"),
          })}
        </p>
      )}
      {!isLoading && !error && (data?.length ?? 0) === 0 && (
        <EmptyState variant="compact" title={t("empty")} />
      )}
      {!isLoading && !error && (data?.length ?? 0) > 0 && (
        <div
          style={{ height: 420, background: isDark ? "#09090b" : "#fafafa" }}
        >
          <ReactFlowProvider>
            <ReactFlow
              nodes={nodes}
              edges={edges}
              nodeTypes={nodeTypes}
              onEdgeClick={onEdgeClick}
              fitView
              fitViewOptions={{ padding: 0.1 }}
              nodesDraggable={false}
              nodesConnectable={false}
              proOptions={{ hideAttribution: true }}
            />
          </ReactFlowProvider>
        </div>
      )}

      {selected && (
        <CrossRefBucketModal
          bucket={selected}
          onClose={() => setSelected(null)}
        />
      )}
    </section>
  );
}

function CrossRefBucketModal({
  bucket,
  onClose,
}: {
  bucket: AggregatedBucket;
  onClose: () => void;
}) {
  const t = useTranslations("ontology.map.crossRef");
  const tAxis = useTranslations("ontology.map.crossRef.axisLabel");

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-labelledby="crossref-bucket-title"
      className="fixed inset-0 z-50 flex items-center justify-center bg-surface-base/40 p-4 backdrop-blur-sm"
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div className="flex max-h-[80vh] w-full max-w-2xl flex-col rounded-lg border border-divider bg-surface-base shadow-xl">
        <header className="flex items-baseline justify-between border-b border-divider px-5 py-3">
          <div>
            <h2
              id="crossref-bucket-title"
              className="text-sm font-semibold text-foreground-strong"
            >
              {tAxis(bucket.source)} → {tAxis(bucket.target)}
            </h2>
            <p className="mt-0.5 text-[11px] text-muted-foreground">
              {t("bucketSubtitle", { count: bucket.count })}
            </p>
          </div>
          <button
            type="button"
            onClick={onClose}
            aria-label={t("close")}
            className="rounded px-2 py-0.5 text-xs text-muted-foreground hover:bg-surface-inset dark:hover:bg-surface-base"
          >
            ✕
          </button>
        </header>
        <ul className="min-h-0 flex-1 divide-y divide-divider-soft overflow-y-auto-soft/60">
          {bucket.edges.map((edge, idx) => (
            <li
              key={`${edge.source_id}-${edge.edge_kind}-${edge.target_id}-${idx}`}
              className="flex flex-col gap-0.5 px-5 py-2 text-xs"
            >
              <div className="flex items-center gap-2">
                <span className="rounded bg-surface-inset px-1.5 py-0.5 font-mono text-2xs text-foreground-muted">
                  {edge.source_kind}
                </span>
                <span className="truncate text-foreground-strong">
                  {edge.source_id}
                </span>
                <span className="text-muted-foreground">—{edge.edge_kind}→</span>
                <span className="rounded bg-surface-inset px-1.5 py-0.5 font-mono text-2xs text-foreground-muted">
                  {edge.target_kind}
                </span>
                <span className="truncate text-foreground-strong">
                  {edge.target_id}
                </span>
              </div>
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
}
