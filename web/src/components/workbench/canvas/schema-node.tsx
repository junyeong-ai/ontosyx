"use client";

import { memo, useEffect, useRef } from "react";
import { useTranslations } from "next-intl";
import { Handle, Position, useStore, useUpdateNodeInternals, type NodeProps } from "@xyflow/react";
import type { NodeTypeDef, PropertyDef, QualityGap } from "@/types/api";
import { formatPropertyType } from "@/types/api";
import { Tooltip } from "@/components/ui/tooltip";
import { cn } from "@/lib/cn";
import { arr } from "@/lib/ir-collections";

type NodeTranslator = ReturnType<typeof useTranslations<"workbench.canvas.node">>;

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
  // Severity-tier counts factor into the canvas overlay so a
  // gap-list reorder that doesn't change total length but shifts
  // severity must still re-render. We compare per-tier counts
  // rather than walking the full gap list because the counts are
  // the only thing the rendered surface reads.
  const aHigh = a.gaps.filter((g) => g.severity === "high").length;
  const bHigh = b.gaps.filter((g) => g.severity === "high").length;
  const aMedium = a.gaps.filter((g) => g.severity === "medium").length;
  const bMedium = b.gaps.filter((g) => g.severity === "medium").length;
  return (
    a.nodeDef.id === b.nodeDef.id &&
    a.nodeDef.label === b.nodeDef.label &&
    arr(a.nodeDef.properties).length === arr(b.nodeDef.properties).length &&
    a.gaps.length === b.gaps.length &&
    aHigh === bHigh &&
    aMedium === bMedium &&
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
    case "problematic": return "bg-danger-solid";
    case "suggested": return "bg-info-surface";
    case "asserted": return "bg-brand-solid";
    default: return "bg-surface-raised dark:bg-surface-base";
  }
}

/** Resolve highlight border classes based on binding kind */
function highlightBorderClass(kind?: import("@/types/api").BindingKind): string {
  switch (kind) {
    case "exists": return "border-concept-foreground ring-2 ring-concept-foreground/20 dark:border-concept-foreground";
    case "path_find": return "border-success-border ring-2 ring-success-foreground dark:border-success-border";
    case "chain": return "border-warning-border ring-2 ring-warning-foreground/30";
    case "mutation": return "border-danger-border ring-2 ring-danger-foreground dark:border-danger-border";
    default: return "border-info-border ring-2 ring-info-foreground/20"; // match + fallback
  }
}

export const SchemaNode = memo(function SchemaNode({ data, id }: SchemaNodeProps) {
  const t = useTranslations("workbench.canvas.node");
  const { nodeDef, gaps, selected, highlighted, highlightKind, highlightedPropertyIds, layer, diffStatus, dimmed, verified } = data;
  // Severity-tiered overlay. `high` is the red-dot tier operators
  // must address before completing the design; `medium` gets the
  // amber treatment; `low` is intentionally suppressed at the canvas
  // surface (still visible inside the inspector's `QualityGapsList`) so
  // the canvas reads as a quality dashboard, not as a wall of
  // yellow noise on every node.
  const highGaps = gaps.filter((g) => g.severity === "high");
  const mediumGaps = gaps.filter((g) => g.severity === "medium");
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
    ? "border-brand-border ring-2 ring-brand-foreground/50 dark:border-brand-foreground"
    : diffStatus === "removed"
      ? "border-danger-border ring-2 ring-danger-foreground/50"
      : diffStatus === "modified"
        ? "border-warning-border ring-2 ring-warning-foreground/30"
        : selected
          ? "border-brand-foreground ring-2 ring-brand-foreground/50"
          : highlighted
            ? highlightBorderClass(highlightKind)
            : hasGaps
              ? "border-warning-border"
              : "border-divider";

  const headerBgClass = diffStatus === "added"
    ? "bg-brand-surface/40"
    : diffStatus === "removed"
      ? "bg-danger-surface/40"
      : diffStatus === "modified"
        ? "bg-warning-surface/40"
        : "bg-brand-surface/40";

  // --- Low detail (zoom < 0.4): compact label only ---
  if (detail === "low") {
    return (
      <div
        ref={containerRef}
        className={cn(
          "relative min-w-[100px] rounded-lg border bg-surface-base shadow-sm",
          borderClass,
          dimmed && "opacity-15 pointer-events-none",
        )}
      >
        <div className={cn("absolute left-0 top-1 bottom-1 w-[3px] rounded-r-full", layerColorClass(layer))} />
        <Handle type="target" position={Position.Left} id={`${nodeDef.id}:left`}
          className="!h-2 !w-2 !border-divider !bg-muted-foreground" />
        <Handle type="source" position={Position.Right} id={`${nodeDef.id}:right`}
          className="!h-2 !w-2 !border-divider !bg-muted-foreground" />
        <Handle type="target" position={Position.Top} id={`${nodeDef.id}:top`}
          className="!h-2 !w-2 !border-divider !bg-muted-foreground" />
        <Handle type="source" position={Position.Bottom} id={`${nodeDef.id}:bottom`}
          className="!h-2 !w-2 !border-divider !bg-muted-foreground" />
        <div className="flex items-center gap-1.5 px-3 py-1.5 overflow-hidden">
          {verified && <span className="h-1.5 w-1.5 shrink-0 rounded-full bg-brand-solid" />}
          <span className="pl-1.5 max-w-full truncate text-xs font-bold tracking-wide text-brand-foreground">
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
          "relative min-w-[140px] rounded-lg border bg-surface-base shadow-sm",
          borderClass,
          dimmed && "opacity-15 pointer-events-none",
        )}
      >
        <div className={cn("absolute left-0 top-2 bottom-2 w-[3px] rounded-r-full", layerColorClass(layer))} />
        {diffStatus && (
          <div
            className={cn(
              "absolute -right-1.5 -top-1.5 rounded-full px-1.5 py-0.5 text-2xs font-bold uppercase leading-none",
              diffStatus === "added" ? "bg-brand-solid text-white" : diffStatus === "removed" ? "bg-danger-solid text-white" : "bg-warning-foreground text-white",
            )}
          >
            {diffStatus === "added" ? t("diffNew") : diffStatus === "removed" ? t("diffRemoved") : t("diffModified")}
          </div>
        )}
        <Handle type="target" position={Position.Left} id={`${nodeDef.id}:left`}
          className="!h-2 !w-2 !border-divider !bg-muted-foreground" />
        <Handle type="source" position={Position.Right} id={`${nodeDef.id}:right`}
          className="!h-2 !w-2 !border-divider !bg-muted-foreground" />
        <Handle type="target" position={Position.Top} id={`${nodeDef.id}:top`}
          className="!h-2 !w-2 !border-divider !bg-muted-foreground" />
        <Handle type="source" position={Position.Bottom} id={`${nodeDef.id}:bottom`}
          className="!h-2 !w-2 !border-divider !bg-muted-foreground" />

        <div className={cn("flex items-center gap-2 rounded-t-lg px-3 py-2 overflow-hidden", headerBgClass)}>
          {verified && <span className="h-1.5 w-1.5 shrink-0 rounded-full bg-brand-solid" />}
          <span className="pl-1.5 min-w-0 truncate text-xs font-bold tracking-wide text-brand-foreground">
            {nodeDef.label}
          </span>
          {highGaps.length > 0 && (
            <span className="flex h-4 w-4 shrink-0 items-center justify-center rounded-full bg-danger-solid text-2xs font-bold text-white">
              {highGaps.length}
            </span>
          )}
          {mediumGaps.length > 0 && highGaps.length === 0 && (
            <span className="flex h-4 w-4 shrink-0 items-center justify-center rounded-full bg-warning-foreground text-2xs font-bold text-white">
              {mediumGaps.length}
            </span>
          )}
        </div>

        {/* Summary badges */}
        <div className="flex items-center gap-2 border-t border-divider-soft px-3 py-1">
          {arr(nodeDef.properties).length > 0 && (
            <span className="pl-1.5 text-2xs text-muted-foreground">
              {t("properties", { count: arr(nodeDef.properties).length })}
            </span>
          )}
          {nodeDef.constraints && arr(nodeDef.constraints).length > 0 && (
            <span className="text-2xs text-muted-foreground">
              {t("constraints", { count: arr(nodeDef.constraints).length })}
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
        "relative min-w-[180px] rounded-lg border bg-surface-base shadow-sm",
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
            "absolute -right-1.5 -top-1.5 rounded-full px-1.5 py-0.5 text-2xs font-bold uppercase leading-none",
            diffStatus === "added"
              ? "bg-brand-solid text-white"
              : "bg-warning-foreground text-white",
          )}
        >
          {diffStatus === "added" ? "NEW" : diffStatus === "removed" ? "DEL" : "MOD"}
        </div>
      )}

      {/* Handles */}
      <Handle type="target" position={Position.Left} id={`${nodeDef.id}:left`}
        className="!h-2 !w-2 !border-divider !bg-muted-foreground" />
      <Handle type="source" position={Position.Right} id={`${nodeDef.id}:right`}
        className="!h-2 !w-2 !border-divider !bg-muted-foreground" />
      <Handle type="target" position={Position.Top} id={`${nodeDef.id}:top`}
        className="!h-2 !w-2 !border-divider !bg-muted-foreground" />
      <Handle type="source" position={Position.Bottom} id={`${nodeDef.id}:bottom`}
        className="!h-2 !w-2 !border-divider !bg-muted-foreground" />

      {/* Header */}
      <div className={cn("flex items-center gap-2 rounded-t-lg px-3 py-2 overflow-hidden", headerBgClass)}>
        {verified && <span className="h-1.5 w-1.5 shrink-0 rounded-full bg-brand-solid" />}
        <span className="pl-1.5 min-w-0 truncate text-xs font-bold tracking-wide text-brand-foreground">
          {nodeDef.label}
        </span>
        {highGaps.length > 0 && (
          <Tooltip content={t("qualityIssues", { count: highGaps.length })}>
            <span className="flex h-4 w-4 shrink-0 items-center justify-center rounded-full bg-danger-solid text-2xs font-bold text-white">
              {highGaps.length}
            </span>
          </Tooltip>
        )}
        {mediumGaps.length > 0 && highGaps.length === 0 && (
          <Tooltip content={t("qualityIssues", { count: mediumGaps.length })}>
            <span className="flex h-4 w-4 shrink-0 items-center justify-center rounded-full bg-warning-foreground text-2xs font-bold text-white">
              {mediumGaps.length}
            </span>
          </Tooltip>
        )}
        {nodeDef.source_lineage?.table && (
          <Tooltip content={t("sourceTooltip", { table: nodeDef.source_lineage?.table })}>
            <span className="ml-auto shrink-0 text-2xs text-muted-foreground">
              {nodeDef.source_lineage?.table}
            </span>
          </Tooltip>
        )}
      </div>

      {/* Properties — separated for independent memoization */}
      {arr(nodeDef.properties).length > 0 && (
        <PropertyList properties={nodeDef.properties} highlightedPropertyIds={highlightedPropertyIds} t={t} />
      )}

      {/* Constraints badge */}
      {nodeDef.constraints && arr(nodeDef.constraints).length > 0 && (
        <div className="border-t border-divider-soft px-3 py-1">
          <span className="pl-1.5 text-2xs text-muted-foreground">
            {t("constraints", { count: arr(nodeDef.constraints).length })}
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
  t,
}: {
  properties: PropertyDef[];
  highlightedPropertyIds?: Set<string>;
  t: NodeTranslator;
}) {
  return (
    <div className="border-t border-divider-soft px-3 py-1.5">
      {properties.slice(0, 8).map((prop) => {
        const isRequired = !prop.nullable;
        return (
          <div key={prop.id} className={cn(
            "flex items-center gap-1.5 py-0.5 text-2xs",
            highlightedPropertyIds?.has(prop.id) && "bg-info-surface/30",
          )}>
            {isRequired && (
              <Tooltip content={t("required")}>
                <span className="text-warning-foreground">*</span>
              </Tooltip>
            )}
            <span className="pl-1.5 text-foreground">{prop.name}</span>
            <span className="ml-auto text-muted-foreground">{formatPropertyType(prop.property_type)}</span>
          </div>
        );
      })}
      {properties.length > 8 && (
        <div className="py-0.5 text-2xs text-muted-foreground">
          {t("moreProperties", { count: properties.length - 8 })}
        </div>
      )}
    </div>
  );
});
