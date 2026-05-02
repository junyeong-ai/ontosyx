import type { NodeVizConfig, GraphLayout } from "@/types/api";
import type { GraphNodeData } from "./graph-types";
import { TYPE_COLORS } from "./graph-constants";
import { formatValue } from "../chart-utils";

// ---------------------------------------------------------------------------
// Color helpers
// ---------------------------------------------------------------------------

export function resolveColorMap(
  nodeConfig: NodeVizConfig | undefined,
): Map<string, string> | null {
  if (!nodeConfig?.color_map?.length) return null;
  const map = new Map<string, string>();
  for (const { value, color } of nodeConfig.color_map) {
    map.set(value, color);
  }
  return map;
}

export function assignNodeColor(
  node: { type?: string; properties: Record<string, unknown> },
  colorField: string | undefined,
  colorMap: Map<string, string> | null,
  typeColorIndex: Map<string, string>,
): string {
  // 1. Use explicit color_map if value matches
  if (colorField && colorMap) {
    const fieldVal = String(node.properties[colorField] ?? node.type ?? "");
    const mapped = colorMap.get(fieldVal);
    if (mapped) return mapped;
  }

  // 2. Use color_field value as lookup key into auto palette
  const typeKey = colorField
    ? String(node.properties[colorField] ?? node.type ?? "default")
    : (node.type ?? "default");

  if (!typeColorIndex.has(typeKey)) {
    typeColorIndex.set(
      typeKey,
      TYPE_COLORS[typeColorIndex.size % TYPE_COLORS.length],
    );
  }
  return typeColorIndex.get(typeKey)!;
}

// ---------------------------------------------------------------------------
// Size helper
// ---------------------------------------------------------------------------

export function assignNodeSize(
  node: { properties: Record<string, unknown> },
  sizeField: string | undefined,
): number {
  if (!sizeField) return 4;
  const val = node.properties[sizeField];
  if (typeof val === "number" && val > 0) {
    // Logarithmic scaling clamped between 2 and 16
    return Math.max(2, Math.min(16, 2 + Math.log2(val + 1) * 2));
  }
  return 4;
}

// ---------------------------------------------------------------------------
// Tooltip helpers
// ---------------------------------------------------------------------------

export function escapeHtml(str: string): string {
  return str
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

export function buildTooltipHtml(
  node: GraphNodeData,
  tooltipFields: string[] | undefined,
): string {
  const fields = tooltipFields?.length ? tooltipFields : Object.keys(node.properties);
  if (!fields.length) return `<b>${escapeHtml(node.label)}</b>`;

  const lines = fields
    .filter((f) => node.properties[f] != null)
    .map((f) => `<b>${escapeHtml(f)}</b>: ${escapeHtml(formatValue(node.properties[f]))}`)
    .join("<br/>");

  return `<div style="font-size:11px;line-height:1.5;max-width:280px">
    <div style="font-weight:600;margin-bottom:2px">${escapeHtml(node.label)}</div>
    ${lines}
  </div>`;
}

// ---------------------------------------------------------------------------
// Layout helper
// ---------------------------------------------------------------------------

export function layoutToDagMode(
  layout: GraphLayout | undefined,
): "td" | "radialout" | undefined {
  switch (layout) {
    case "hierarchical":
    case "dagre":
      return "td";
    case "radial":
      return "radialout";
    default:
      return undefined; // force layout = default
  }
}
