"use client";

import { memo } from "react";
import {
  BaseEdge,
  EdgeLabelRenderer,
  getBezierPath,
  type EdgeProps,
} from "@xyflow/react";
import type { Cardinality, EdgeTypeDef } from "@/types/api";
import { cn } from "@/lib/cn";
import type { DiffStatus } from "./schema-node";

// ---------------------------------------------------------------------------
// Schema edge — renders a relationship type on the canvas
// ---------------------------------------------------------------------------

export interface SchemaEdgeData {
  edgeDef: EdgeTypeDef;
  selected: boolean;
  highlighted: boolean;
  highlightKind?: import("@/types/api").BindingKind;
  diffStatus?: DiffStatus;
}

type SchemaEdgeProps = EdgeProps & { data: SchemaEdgeData };

function schemaEdgeEqual(prev: SchemaEdgeProps, next: SchemaEdgeProps): boolean {
  const a = prev.data;
  const b = next.data;
  return (
    prev.sourceX === next.sourceX &&
    prev.sourceY === next.sourceY &&
    prev.targetX === next.targetX &&
    prev.targetY === next.targetY &&
    a?.edgeDef?.id === b?.edgeDef?.id &&
    a?.edgeDef?.label === b?.edgeDef?.label &&
    a?.selected === b?.selected &&
    a?.highlighted === b?.highlighted &&
    a?.highlightKind === b?.highlightKind &&
    a?.diffStatus === b?.diffStatus
  );
}

function highlightStrokeColor(kind?: import("@/types/api").BindingKind): string {
  switch (kind) {
    case "exists": return "#a78bfa";   // violet-400
    case "path_find": return "#22d3ee"; // cyan-400
    case "chain": return "#fbbf24";     // amber-400
    case "mutation": return "#fb7185";   // rose-400
    default: return "#38bdf8";           // sky-400 (match + fallback)
  }
}

function highlightLabelClass(kind?: import("@/types/api").BindingKind): string {
  switch (kind) {
    case "exists": return "bg-violet-100 text-violet-700 dark:bg-violet-900 dark:text-violet-300";
    case "path_find": return "bg-cyan-100 text-cyan-700 dark:bg-cyan-900 dark:text-cyan-300";
    case "chain": return "bg-amber-100 text-amber-700 dark:bg-amber-900 dark:text-amber-300";
    case "mutation": return "bg-rose-100 text-rose-700 dark:bg-rose-900 dark:text-rose-300";
    default: return "bg-sky-100 text-sky-700 dark:bg-sky-900 dark:text-sky-300";
  }
}

export const SchemaEdge = memo(function SchemaEdge({
  id,
  sourceX,
  sourceY,
  targetX,
  targetY,
  sourcePosition,
  targetPosition,
  data,
  style,
  markerEnd,
}: SchemaEdgeProps) {
  const { edgeDef, selected, highlighted, highlightKind, diffStatus } = data ?? {};

  const [edgePath, labelX, labelY] = getBezierPath({
    sourceX,
    sourceY,
    sourcePosition,
    targetX,
    targetY,
    targetPosition,
  });

  const strokeColor = diffStatus === "added"
    ? "#10b981" // emerald
    : diffStatus === "modified"
      ? "#f59e0b" // amber
      : selected
        ? "#10b981"
        : highlighted
          ? highlightStrokeColor(highlightKind)
          : "#94a3b8";

  const strokeWidth = diffStatus || selected || highlighted ? 2.5 : 1.5;
  const dashArray = diffStatus === "added" ? "6 3" : undefined;

  return (
    <>
      <BaseEdge
        id={id}
        path={edgePath}
        markerEnd={markerEnd}
        style={{
          ...style,
          strokeWidth,
          stroke: strokeColor,
          strokeDasharray: dashArray,
        }}
      />
      <EdgeLabelRenderer>
        <div
          className={cn(
            "nodrag nopan pointer-events-auto absolute rounded-md px-1.5 py-0.5 text-[10px] font-medium",
            diffStatus === "added"
              ? "bg-emerald-100 text-emerald-700 dark:bg-emerald-900 dark:text-emerald-300"
              : diffStatus === "modified"
                ? "bg-amber-100 text-amber-700 dark:bg-amber-900 dark:text-amber-300"
                : selected
                  ? "bg-emerald-100 text-emerald-700 dark:bg-emerald-900 dark:text-emerald-300"
                  : highlighted
                    ? highlightLabelClass(highlightKind)
                    : "bg-white text-zinc-500 shadow-sm dark:bg-zinc-800 dark:text-zinc-400",
          )}
          style={{
            transform: `translate(-50%, -50%) translate(${labelX}px, ${labelY}px)`,
          }}
        >
          {diffStatus && (
            <span className={cn(
              "mr-1 text-[8px] font-bold uppercase",
              diffStatus === "added" ? "text-emerald-500" : "text-amber-500",
            )}>
              {diffStatus === "added" ? "+" : "~"}
            </span>
          )}
          {edgeDef?.label ?? id}
          {edgeDef?.cardinality && edgeDef.cardinality !== "many_to_many" && (
            <span className="ml-1 text-[8px] text-zinc-400">
              ({formatCardinality(edgeDef.cardinality)})
            </span>
          )}
        </div>
      </EdgeLabelRenderer>
    </>
  );
}, schemaEdgeEqual);

function formatCardinality(c: Cardinality): string {
  switch (c) {
    case "one_to_one": return "1:1";
    case "one_to_many": return "1:N";
    case "many_to_one": return "N:1";
    default: return "N:N";
  }
}
