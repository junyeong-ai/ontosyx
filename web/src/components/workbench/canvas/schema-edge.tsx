"use client";

import { memo } from "react";
import {
  BaseEdge,
  EdgeLabelRenderer,
  getBezierPath,
  type EdgeProps,
} from "@xyflow/react";
import type { Cardinality, EdgeTypeDef, QualityGap } from "@/types/api";
import { cn } from "@/lib/cn";
import { localize } from "@/lib/locale/localize";
import { useLocaleChain } from "@/lib/use-locale-chain";
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
  /** ADR-0057 — quality gaps the runtime / SHACL flagged for this
   *  edge. Renders as a severity-tiered dot adjacent to the edge
   *  label (red for `high`, amber for `medium`); `low` gaps are
   *  suppressed at the canvas surface to keep it readable, and
   *  remain visible inside the inspector's `GapsList`. */
  gaps?: QualityGap[];
}

type SchemaEdgeProps = EdgeProps & { data: SchemaEdgeData };

function schemaEdgeEqual(prev: SchemaEdgeProps, next: SchemaEdgeProps): boolean {
  const a = prev.data;
  const b = next.data;
  // Mirror SchemaNode's per-tier counting (ADR-0057) so a gap-list
  // reorder that doesn't change total length but shifts severity
  // still re-renders.
  const aHigh = (a?.gaps ?? []).filter((g) => g.severity === "high").length;
  const bHigh = (b?.gaps ?? []).filter((g) => g.severity === "high").length;
  const aMedium = (a?.gaps ?? []).filter((g) => g.severity === "medium").length;
  const bMedium = (b?.gaps ?? []).filter((g) => g.severity === "medium").length;
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
    a?.diffStatus === b?.diffStatus &&
    aHigh === bHigh &&
    aMedium === bMedium
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
  const { edgeDef, selected, highlighted, highlightKind, diffStatus, gaps } = data ?? {};
  const highGapCount =
    gaps?.filter((g) => g.severity === "high").length ?? 0;
  const mediumGapCount =
    gaps?.filter((g) => g.severity === "medium").length ?? 0;
  // The locale chain is shared across all edge instances via the
  // hook's TanStack Query cache — N edges resolve to one fetch.
  const localeChain = useLocaleChain();

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

  const hoverTitle = buildHoverTitle(edgeDef, localeChain);

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
          title={hoverTitle}
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
                    : "bg-white text-zinc-500 shadow-sm dark:bg-zinc-800 dark:text-muted-foreground",
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
            <span className="ml-1 text-[8px] text-muted-foreground">
              ({formatCardinality(edgeDef.cardinality)})
            </span>
          )}
          {highGapCount > 0 && (
            <span
              className="ml-1 inline-flex h-3 min-w-3 items-center justify-center rounded-full bg-red-500 px-0.5 text-[8px] font-bold text-white"
              aria-label={`${highGapCount} high-severity quality gaps`}
            >
              {highGapCount}
            </span>
          )}
          {mediumGapCount > 0 && highGapCount === 0 && (
            <span
              className="ml-1 inline-flex h-3 min-w-3 items-center justify-center rounded-full bg-amber-500 px-0.5 text-[8px] font-bold text-white"
              aria-label={`${mediumGapCount} medium-severity quality gaps`}
            >
              {mediumGapCount}
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

/** Build the multi-line hover summary the browser shows as the
 *  edge tooltip. Lines stay compact so the native tooltip stays
 *  readable; description gets truncated past 200 chars to prevent
 *  a wall-of-text on richly-documented edges. */
function buildHoverTitle(
  edgeDef: EdgeTypeDef | undefined,
  chain: readonly string[],
): string | undefined {
  if (!edgeDef) return undefined;
  const lines: string[] = [];
  lines.push(edgeDef.label);
  // Source/target ids stay raw — looking them up to labels would
  // require threading the ontology snapshot into every edge
  // render (the current data flow keeps the edge component
  // standalone). The ids are the same shape OntologyValidator
  // reports so an admin recognises them.
  lines.push(`${edgeDef.source_node_id} → ${edgeDef.target_node_id}`);
  if (edgeDef.cardinality) {
    lines.push(`Cardinality: ${formatCardinality(edgeDef.cardinality)}`);
  }
  const description = localize(edgeDef.description, chain);
  if (description) {
    const trimmed =
      description.length > 200 ? `${description.slice(0, 200)}…` : description;
    lines.push("");
    lines.push(trimmed);
  }
  return lines.join("\n");
}
