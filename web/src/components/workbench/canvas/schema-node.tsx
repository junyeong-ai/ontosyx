"use client";

import { memo, useEffect, useRef } from "react";
import { Handle, Position, useStore, useUpdateNodeInternals, type NodeProps } from "@xyflow/react";
import type { NodeTypeDef, PropertyDef, QualityGap } from "@/types/api";
import { formatPropertyType } from "@/types/api";
import { Tooltip } from "@/components/ui/tooltip";
import { cn } from "@/lib/cn";

// ---------------------------------------------------------------------------
// Schema node — renders a graph node type on the canvas
// ---------------------------------------------------------------------------

export type NodeLayer = "asserted" | "inferred" | "suggested" | "problematic";
export type DiffStatus = "added" | "modified" | "removed";

/** Semantic zoom detail level */
type DetailLevel = "low" | "medium" | "high";

function getDetailLevel(zoom: number): DetailLevel {
  if (zoom < 0.4) return "low";
  if (zoom <= 0.8) return "medium";
  return "high";
}

export interface SchemaNodeData {
  nodeDef: NodeTypeDef;
  gaps: QualityGap[];
  selected: boolean;
  highlighted: boolean;
  /** Binding kind for provenance-aware highlighting */
  highlightKind?: import("@/types/api").BindingKind;
  /** Property IDs referenced in the query */
  highlightedPropertyIds?: Set<string>;
  layer: NodeLayer;
  diffStatus?: DiffStatus;
  /** Set by neighborhood mode — dims non-focused nodes */
  dimmed?: boolean;
  verified?: boolean;
}

type SchemaNodeProps = NodeProps & { data: SchemaNodeData };

function schemaNodeEqual(prev: SchemaNodeProps, next: SchemaNodeProps): boolean {
  const a = prev.data;
  const b = next.data;
  return (
    a.nodeDef.id === b.nodeDef.id &&
    a.nodeDef.label === b.nodeDef.label &&
    a.nodeDef.properties.length === b.nodeDef.properties.length &&
    a.gaps.length === b.gaps.length &&
    a.highlighted === b.highlighted &&
    a.highlightKind === b.highlightKind &&
    a.highlightedPropertyIds === b.highlightedPropertyIds &&
    a.selected === b.selected &&
    a.diffStatus === b.diffStatus &&
    a.layer === b.layer &&
    a.dimmed === b.dimmed &&
    a.verified === b.verified
  );
}

/** Resolve the layer indicator color class */
function layerColorClass(layer: NodeLayer): string {
  switch (layer) {
    case "problematic": return "bg-red-500";
    case "suggested": return "bg-sky-500";
    case "asserted": return "bg-emerald-500";
    default: return "bg-zinc-300 dark:bg-zinc-600";
  }
}

/** Resolve highlight border classes based on binding kind */
function highlightBorderClass(kind?: import("@/types/api").BindingKind): string {
  switch (kind) {
    case "exists": return "border-violet-400 ring-2 ring-violet-400/30 dark:border-violet-500";
    case "path_find": return "border-cyan-400 ring-2 ring-cyan-400/30 dark:border-cyan-500";
    case "chain": return "border-amber-400 ring-2 ring-amber-400/30 dark:border-amber-500";
    case "mutation": return "border-rose-400 ring-2 ring-rose-400/30 dark:border-rose-500";
    default: return "border-sky-400 ring-2 ring-sky-400/30 dark:border-sky-500"; // match + fallback
  }
}

export const SchemaNode = memo(function SchemaNode({ data, id }: SchemaNodeProps) {
  const { nodeDef, gaps, selected, highlighted, highlightKind, highlightedPropertyIds, layer, diffStatus, dimmed, verified } = data;
  const highGaps = gaps.filter((g) => g.severity === "high");
  const hasGaps = gaps.length > 0;

  // Semantic zoom — read current zoom from React Flow store
  const zoom = useStore((s) => s.transform[2]);
  const detail = getDetailLevel(zoom);

  // Update measured dimensions when detail level crosses thresholds
  const containerRef = useRef<HTMLDivElement>(null);
  const updateNodeInternals = useUpdateNodeInternals();
  const prevDetailRef = useRef<DetailLevel>(detail);

  useEffect(() => {
    if (prevDetailRef.current !== detail) {
      prevDetailRef.current = detail;
      updateNodeInternals(id);
    }
  }, [detail, id, updateNodeInternals]);

  const borderClass = diffStatus === "added"
    ? "border-emerald-400 ring-2 ring-emerald-400/50 dark:border-emerald-500"
    : diffStatus === "removed"
      ? "border-red-400 ring-2 ring-red-400/50 dark:border-red-500"
      : diffStatus === "modified"
        ? "border-amber-400 ring-2 ring-amber-400/30 dark:border-amber-500"
        : selected
          ? "border-emerald-500 ring-2 ring-emerald-500/50"
          : highlighted
            ? highlightBorderClass(highlightKind)
            : hasGaps
              ? "border-amber-300 dark:border-amber-600"
              : "border-zinc-200 dark:border-zinc-700";

  const headerBgClass = diffStatus === "added"
    ? "bg-emerald-50 dark:bg-emerald-950/40"
    : diffStatus === "removed"
      ? "bg-red-50 dark:bg-red-950/40"
      : diffStatus === "modified"
        ? "bg-amber-50 dark:bg-amber-950/40"
        : "bg-emerald-50 dark:bg-emerald-950/40";

  // --- Low detail (zoom < 0.4): compact label only ---
  if (detail === "low") {
    return (
      <div
        ref={containerRef}
        className={cn(
          "relative min-w-[100px] rounded-lg border bg-white shadow-sm dark:bg-zinc-900",
          borderClass,
          dimmed && "opacity-15 pointer-events-none",
        )}
      >
        <div className={cn("absolute left-0 top-1 bottom-1 w-[3px] rounded-r-full", layerColorClass(layer))} />
        <Handle type="target" position={Position.Left} id={`${nodeDef.id}:left`}
          className="!h-2 !w-2 !border-zinc-300 !bg-zinc-400" />
        <Handle type="source" position={Position.Right} id={`${nodeDef.id}:right`}
          className="!h-2 !w-2 !border-zinc-300 !bg-zinc-400" />
        <Handle type="target" position={Position.Top} id={`${nodeDef.id}:top`}
          className="!h-2 !w-2 !border-zinc-300 !bg-zinc-400" />
        <Handle type="source" position={Position.Bottom} id={`${nodeDef.id}:bottom`}
          className="!h-2 !w-2 !border-zinc-300 !bg-zinc-400" />
        <div className="flex items-center gap-1.5 px-3 py-1.5 overflow-hidden">
          {verified && <span className="h-1.5 w-1.5 shrink-0 rounded-full bg-emerald-500" />}
          <span className="pl-1.5 max-w-full truncate text-xs font-bold tracking-wide text-emerald-700 dark:text-emerald-400">
            {nodeDef.label}
          </span>
        </div>
      </div>
    );
  }

  // --- Medium detail (zoom 0.4-0.8): label + count badges ---
  if (detail === "medium") {
    return (
      <div
        ref={containerRef}
        className={cn(
          "relative min-w-[140px] rounded-lg border bg-white shadow-sm dark:bg-zinc-900",
          borderClass,
          dimmed && "opacity-15 pointer-events-none",
        )}
      >
        <div className={cn("absolute left-0 top-2 bottom-2 w-[3px] rounded-r-full", layerColorClass(layer))} />
        {diffStatus && (
          <div
            className={cn(
              "absolute -right-1.5 -top-1.5 rounded-full px-1.5 py-0.5 text-[8px] font-bold uppercase leading-none",
              diffStatus === "added" ? "bg-emerald-500 text-white" : diffStatus === "removed" ? "bg-red-500 text-white" : "bg-amber-500 text-white",
            )}
          >
            {diffStatus === "added" ? "NEW" : diffStatus === "removed" ? "DEL" : "MOD"}
          </div>
        )}
        <Handle type="target" position={Position.Left} id={`${nodeDef.id}:left`}
          className="!h-2 !w-2 !border-zinc-300 !bg-zinc-400" />
        <Handle type="source" position={Position.Right} id={`${nodeDef.id}:right`}
          className="!h-2 !w-2 !border-zinc-300 !bg-zinc-400" />
        <Handle type="target" position={Position.Top} id={`${nodeDef.id}:top`}
          className="!h-2 !w-2 !border-zinc-300 !bg-zinc-400" />
        <Handle type="source" position={Position.Bottom} id={`${nodeDef.id}:bottom`}
          className="!h-2 !w-2 !border-zinc-300 !bg-zinc-400" />

        <div className={cn("flex items-center gap-2 rounded-t-lg px-3 py-2 overflow-hidden", headerBgClass)}>
          {verified && <span className="h-1.5 w-1.5 shrink-0 rounded-full bg-emerald-500" />}
          <span className="pl-1.5 min-w-0 truncate text-xs font-bold tracking-wide text-emerald-700 dark:text-emerald-400">
            {nodeDef.label}
          </span>
          {highGaps.length > 0 && (
            <span className="flex h-4 w-4 shrink-0 items-center justify-center rounded-full bg-amber-500 text-[9px] font-bold text-white">
              {highGaps.length}
            </span>
          )}
        </div>

        {/* Summary badges */}
        <div className="flex items-center gap-2 border-t border-zinc-100 px-3 py-1 dark:border-zinc-800">
          {nodeDef.properties.length > 0 && (
            <span className="pl-1.5 text-[9px] text-zinc-400">
              {nodeDef.properties.length} prop{nodeDef.properties.length > 1 ? "s" : ""}
            </span>
          )}
          {nodeDef.constraints && nodeDef.constraints.length > 0 && (
            <span className="text-[9px] text-zinc-400">
              {nodeDef.constraints.length} constraint{nodeDef.constraints.length > 1 ? "s" : ""}
            </span>
          )}
        </div>
      </div>
    );
  }

  // --- High detail (zoom > 0.8): full view ---
  return (
    <div
      ref={containerRef}
      className={cn(
        "relative min-w-[180px] rounded-lg border bg-white shadow-sm dark:bg-zinc-900",
        borderClass,
        dimmed && "opacity-15 pointer-events-none",
      )}
    >
      {/* Layer indicator bar (left edge) */}
      <div className={cn("absolute left-0 top-2 bottom-2 w-[3px] rounded-r-full", layerColorClass(layer))} />

      {/* Diff badge */}
      {diffStatus && (
        <div
          className={cn(
            "absolute -right-1.5 -top-1.5 rounded-full px-1.5 py-0.5 text-[8px] font-bold uppercase leading-none",
            diffStatus === "added"
              ? "bg-emerald-500 text-white"
              : "bg-amber-500 text-white",
          )}
        >
          {diffStatus === "added" ? "NEW" : diffStatus === "removed" ? "DEL" : "MOD"}
        </div>
      )}

      {/* Handles */}
      <Handle type="target" position={Position.Left} id={`${nodeDef.id}:left`}
        className="!h-2 !w-2 !border-zinc-300 !bg-zinc-400" />
      <Handle type="source" position={Position.Right} id={`${nodeDef.id}:right`}
        className="!h-2 !w-2 !border-zinc-300 !bg-zinc-400" />
      <Handle type="target" position={Position.Top} id={`${nodeDef.id}:top`}
        className="!h-2 !w-2 !border-zinc-300 !bg-zinc-400" />
      <Handle type="source" position={Position.Bottom} id={`${nodeDef.id}:bottom`}
        className="!h-2 !w-2 !border-zinc-300 !bg-zinc-400" />

      {/* Header */}
      <div className={cn("flex items-center gap-2 rounded-t-lg px-3 py-2 overflow-hidden", headerBgClass)}>
        {verified && <span className="h-1.5 w-1.5 shrink-0 rounded-full bg-emerald-500" />}
        <span className="pl-1.5 min-w-0 truncate text-xs font-bold tracking-wide text-emerald-700 dark:text-emerald-400">
          {nodeDef.label}
        </span>
        {highGaps.length > 0 && (
          <Tooltip content={`${highGaps.length} quality issue(s)`}>
            <span className="flex h-4 w-4 shrink-0 items-center justify-center rounded-full bg-amber-500 text-[9px] font-bold text-white">
              {highGaps.length}
            </span>
          </Tooltip>
        )}
        {nodeDef.source_table && (
          <Tooltip content={`Source: ${nodeDef.source_table}`}>
            <span className="ml-auto shrink-0 text-[9px] text-zinc-400">
              {nodeDef.source_table}
            </span>
          </Tooltip>
        )}
      </div>

      {/* Properties — separated for independent memoization */}
      {nodeDef.properties.length > 0 && (
        <PropertyList properties={nodeDef.properties} highlightedPropertyIds={highlightedPropertyIds} />
      )}

      {/* Constraints badge */}
      {nodeDef.constraints && nodeDef.constraints.length > 0 && (
        <div className="border-t border-zinc-100 px-3 py-1 dark:border-zinc-800">
          <span className="pl-1.5 text-[9px] text-zinc-400">
            {nodeDef.constraints.length} constraint{nodeDef.constraints.length > 1 ? "s" : ""}
          </span>
        </div>
      )}
    </div>
  );
}, schemaNodeEqual);

// ---------------------------------------------------------------------------
// PropertyList — memoized to avoid re-rendering when only selection changes
// ---------------------------------------------------------------------------

const PropertyList = memo(function PropertyList({
  properties,
  highlightedPropertyIds,
}: {
  properties: PropertyDef[];
  highlightedPropertyIds?: Set<string>;
}) {
  return (
    <div className="border-t border-zinc-100 px-3 py-1.5 dark:border-zinc-800">
      {properties.slice(0, 8).map((prop) => {
        const isRequired = !prop.nullable;
        return (
          <div key={prop.id} className={cn(
            "flex items-center gap-1.5 py-0.5 text-[10px]",
            highlightedPropertyIds?.has(prop.id) && "bg-sky-50 dark:bg-sky-950/30",
          )}>
            {isRequired && (
              <Tooltip content="Required">
                <span className="text-amber-500">*</span>
              </Tooltip>
            )}
            <span className="pl-1.5 text-zinc-700 dark:text-zinc-300">{prop.name}</span>
            <span className="ml-auto text-zinc-400">{formatPropertyType(prop.property_type)}</span>
          </div>
        );
      })}
      {properties.length > 8 && (
        <div className="py-0.5 text-[9px] text-zinc-400">
          +{properties.length - 8} more
        </div>
      )}
    </div>
  );
});
