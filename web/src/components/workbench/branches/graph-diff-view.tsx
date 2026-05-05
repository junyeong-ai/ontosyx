"use client";

import { useMemo } from "react";
import { useTranslations } from "next-intl";

import type {
  DiffAddedEdge,
  DiffAddedNode,
  DiffModifiedEdge,
  DiffModifiedNode,
  OntologyDiffSummary,
} from "@/types/ontology-branches";

// ---------------------------------------------------------------------------
// Side-by-side graph diff — compact SVG visualization of `OntologyDiff`
//
// Operators reading the chip list see *what* changed; this view
// answers *where in the graph* the changes cluster. Each node gets
// a colored disc (added=success, removed=danger, modified=info),
// each edge a colored line, and the legend names the four
// categories. Layout is a deterministic grid — no force / ELK
// dependency — so the rendering is stable across remounts and
// serializes cleanly into screenshots.
//
// The full canvas-level diff overlay (XyFlow nodes drawn on the
// design canvas, snap-to-real-position, hover inspectors) lives
// on a separate surface; this view exists so the operator can
// triage the structural shape of the change without leaving the
// branches dashboard.
// ---------------------------------------------------------------------------

type NodeStatus = "added" | "removed" | "modified";
type EdgeStatus = "added" | "removed" | "modified";

interface NodeNode {
  id: string;
  label: string;
  status: NodeStatus;
  /** Layout column slot — drives x position. */
  col: number;
  /** Layout row slot — drives y position within the column. */
  row: number;
}

interface NodeEdge {
  id: string;
  label: string;
  status: EdgeStatus;
  source_id: string;
  target_id: string;
}

const NODE_RADIUS = 22;
const COL_WIDTH = 200;
const ROW_HEIGHT = 90;
const X_PADDING = 60;
const Y_PADDING = 40;

const STATUS_COLOR: Record<NodeStatus, { fill: string; stroke: string; text: string }> = {
  added: {
    fill: "var(--color-success-surface)",
    stroke: "var(--color-success-foreground)",
    text: "var(--color-success-foreground)",
  },
  removed: {
    fill: "var(--color-danger-surface)",
    stroke: "var(--color-danger-foreground)",
    text: "var(--color-danger-foreground)",
  },
  modified: {
    fill: "var(--color-info-surface)",
    stroke: "var(--color-info-foreground)",
    text: "var(--color-info-foreground)",
  },
};

/**
 * Lay nodes out in three deterministic columns: removed (left),
 * modified (center), added (right). Within a column nodes stack
 * top-to-bottom in the order the BE emitted them. Stable across
 * renders, easy to read, no layout fighting.
 */
function layoutNodes(diff: OntologyDiffSummary): NodeNode[] {
  const out: NodeNode[] = [];
  const removed = (diff.removed_nodes ?? []) as DiffAddedNode[];
  const modified = (diff.modified_nodes ?? []) as DiffModifiedNode[];
  const added = (diff.added_nodes ?? []) as DiffAddedNode[];
  removed.forEach((n, idx) => {
    out.push({
      id: n.id,
      label: n.label || n.id,
      status: "removed",
      col: 0,
      row: idx,
    });
  });
  modified.forEach((n, idx) => {
    out.push({
      id: n.node_id,
      label: n.label || n.node_id,
      status: "modified",
      col: 1,
      row: idx,
    });
  });
  added.forEach((n, idx) => {
    out.push({
      id: n.id,
      label: n.label || n.id,
      status: "added",
      col: 2,
      row: idx,
    });
  });
  return out;
}

function layoutEdges(diff: OntologyDiffSummary): NodeEdge[] {
  const out: NodeEdge[] = [];
  for (const e of (diff.removed_edges ?? []) as DiffAddedEdge[]) {
    const src = (e.source_node_id as string | undefined) ?? "";
    const tgt = (e.target_node_id as string | undefined) ?? "";
    out.push({
      id: e.id,
      label: e.label || e.id,
      status: "removed",
      source_id: src,
      target_id: tgt,
    });
  }
  for (const e of (diff.modified_edges ?? []) as DiffModifiedEdge[]) {
    out.push({
      id: e.edge_id,
      label: e.label || e.edge_id,
      status: "modified",
      source_id: "",
      target_id: "",
    });
  }
  for (const e of (diff.added_edges ?? []) as DiffAddedEdge[]) {
    const src = (e.source_node_id as string | undefined) ?? "";
    const tgt = (e.target_node_id as string | undefined) ?? "";
    out.push({
      id: e.id,
      label: e.label || e.id,
      status: "added",
      source_id: src,
      target_id: tgt,
    });
  }
  return out;
}

interface GraphDiffViewProps {
  diff: OntologyDiffSummary;
}

export function GraphDiffView({ diff }: GraphDiffViewProps) {
  const t = useTranslations("workbench.branches.graphDiff");
  const nodes = useMemo(() => layoutNodes(diff), [diff]);
  const edges = useMemo(() => layoutEdges(diff), [diff]);
  const nodeMap = useMemo(() => {
    const m = new Map<string, NodeNode>();
    for (const n of nodes) m.set(n.id, n);
    return m;
  }, [nodes]);

  if (nodes.length === 0 && edges.length === 0) {
    return (
      <p className="text-sm text-foreground-muted">{t("empty")}</p>
    );
  }

  const colMaxRows = [0, 1, 2].map((c) =>
    nodes.filter((n) => n.col === c).length,
  );
  const maxRows = Math.max(...colMaxRows, 1);
  const width = X_PADDING * 2 + COL_WIDTH * 3;
  const height = Y_PADDING * 2 + Math.max(maxRows, 1) * ROW_HEIGHT;

  const nodeXY = (n: NodeNode) => ({
    x: X_PADDING + n.col * COL_WIDTH + COL_WIDTH / 2,
    y: Y_PADDING + n.row * ROW_HEIGHT + ROW_HEIGHT / 2,
  });

  return (
    <div className="overflow-auto rounded-xl border border-divider bg-surface-base p-3">
      <div className="mb-3 flex items-center gap-3 text-2xs">
        <Legend status="removed" label={t("legend.removed")} />
        <Legend status="modified" label={t("legend.modified")} />
        <Legend status="added" label={t("legend.added")} />
        <span className="ml-auto text-foreground-muted tabular-nums">
          {t("counts", {
            nodes: nodes.length,
            edges: edges.length,
          })}
        </span>
      </div>
      <svg
        width={width}
        height={height}
        viewBox={`0 0 ${width} ${height}`}
        role="img"
        aria-label={t("ariaLabel")}
      >
        {/* Column headers */}
        {[
          [0, t("column.removed")],
          [1, t("column.modified")],
          [2, t("column.added")],
        ].map(([col, label]) => (
          <text
            key={String(col)}
            x={X_PADDING + (col as number) * COL_WIDTH + COL_WIDTH / 2}
            y={Y_PADDING - 16}
            textAnchor="middle"
            className="fill-foreground-muted text-[11px] font-medium uppercase tracking-wide"
          >
            {label}
          </text>
        ))}

        {/* Edges — render only those whose endpoints we placed (added/removed
            edges with both endpoints in the node grid). Modified edges
            go in the legend count without a line because the BE
            doesn't emit endpoint deltas for them at this layer. */}
        {edges.map((e) => {
          const src = nodeMap.get(e.source_id);
          const tgt = nodeMap.get(e.target_id);
          if (!src || !tgt) return null;
          const a = nodeXY(src);
          const b = nodeXY(tgt);
          const colors = STATUS_COLOR[e.status];
          return (
            <line
              key={e.id}
              x1={a.x}
              y1={a.y}
              x2={b.x}
              y2={b.y}
              stroke={colors.stroke}
              strokeWidth={1.5}
              strokeDasharray={e.status === "modified" ? "4 3" : undefined}
            />
          );
        })}

        {/* Nodes */}
        {nodes.map((n) => {
          const { x, y } = nodeXY(n);
          const colors = STATUS_COLOR[n.status];
          return (
            <g key={n.id}>
              <circle
                cx={x}
                cy={y}
                r={NODE_RADIUS}
                fill={colors.fill}
                stroke={colors.stroke}
                strokeWidth={1.5}
              />
              <text
                x={x}
                y={y + NODE_RADIUS + 14}
                textAnchor="middle"
                className="fill-foreground text-[11px] font-medium"
              >
                {n.label.length > 18 ? `${n.label.slice(0, 17)}…` : n.label}
              </text>
            </g>
          );
        })}
      </svg>
    </div>
  );
}

function Legend({
  status,
  label,
}: {
  status: NodeStatus;
  label: string;
}) {
  const colors = STATUS_COLOR[status];
  return (
    <span className="inline-flex items-center gap-1.5">
      <span
        aria-hidden
        className="inline-block h-2.5 w-2.5 rounded-full"
        style={{ background: colors.fill, border: `1.5px solid ${colors.stroke}` }}
      />
      <span className="text-foreground-muted">{label}</span>
    </span>
  );
}
