import type { CSSProperties } from "react";
import type { QueryResult, WidgetSpec } from "@/types/api";

// ---------------------------------------------------------------------------
// Color palettes — single source of truth for all chart & widget rendering
// ---------------------------------------------------------------------------

/**
 * 30-color palette for charts and graph widgets.
 * Designed for perceptual distinctness at both small and large category counts.
 * Shared across bar, pie, line, combo, and graph widgets.
 */
export const PALETTE_PRIMARY = [
  // Core (high contrast, distinct hues)
  "#059669", "#2563eb", "#8b5cf6", "#db2777", "#ea580c",
  "#0891b2", "#d97706", "#7c3aed", "#0d9488", "#e11d48",
  // Extended (secondary hues for 10+ categories)
  "#16a34a", "#0ea5e9", "#a855f7", "#f43f5e", "#65a30d",
  "#06b6d4", "#f59e0b", "#ec4899", "#4f46e5", "#0284c7",
  // Deep (tertiary for 20+ categories)
  "#047857", "#1d4ed8", "#6d28d9", "#be123c", "#c2410c",
  "#0e7490", "#b45309", "#9333ea", "#115e59", "#9f1239",
];

/** Secondary palette for overlaid lines in combo charts */
export const PALETTE_SECONDARY = [
  "#f59e0b", "#ef4444", "#ec4899", "#8b5cf6",
];

// ---------------------------------------------------------------------------
// Dark-mode-aware style helpers for Recharts
//
// Recharts uses direct SVG attributes (not CSS), so we must pass computed
// color values. These helpers accept an `isDark` boolean from useIsDarkMode().
// ---------------------------------------------------------------------------

/** Axis tick style (label text on X/Y axes) */
export function axisTickStyle(isDark: boolean) {
  return { fontSize: 11, fill: isDark ? "#a1a1aa" : "#71717a" } as const;
}

/** Axis line stroke color */
export function axisLineStroke(isDark: boolean): string {
  return isDark ? "#3f3f46" : "#d4d4d8";
}

/** Grid line stroke color */
export function gridStroke(isDark: boolean): string {
  return isDark ? "#27272a" : "#e4e4e7";
}

/** Tooltip container style */
export function tooltipStyle(isDark: boolean): CSSProperties {
  return {
    fontSize: 12,
    borderRadius: 8,
    border: `1px solid ${isDark ? "#3f3f46" : "#e4e4e7"}`,
    boxShadow: "0 4px 6px -1px rgb(0 0 0 / 0.1)",
    backgroundColor: isDark ? "#18181b" : "#ffffff",
    color: isDark ? "#e4e4e7" : "#3f3f46",
  };
}

/** Pie chart label fill */
export function pieLabelFill(isDark: boolean): string {
  return isDark ? "#a1a1aa" : "#71717a";
}

/** Pie chart label line stroke */
export function pieLabelLineStroke(isDark: boolean): string {
  return isDark ? "#52525b" : "#a1a1aa";
}


/**
 * Threshold for category count.
 * Used for: X-axis label rotation, pie chart legend visibility, auto-detect pie vs bar.
 */
export const CATEGORY_THRESHOLD = 8;

/** Maximum bar width in pixels */
export const MAX_BAR_SIZE = 48;

// ---------------------------------------------------------------------------
// Field resolution — extracts axis/value fields from spec with fallback
// ---------------------------------------------------------------------------

/** Resolve the label (X-axis / name) field — prefers the string column */
export function resolveLabelField(spec: WidgetSpec, data: QueryResult): string | undefined {
  if (spec.x_axis?.field) return spec.x_axis.field;
  if (spec.data_mapping?.label) return spec.data_mapping.label;

  // Auto-detect: pick the first string column as label
  if (data.columns.length >= 2 && data.rows.length > 0) {
    const first = data.rows[0];
    for (const col of data.columns) {
      if (typeof first[col] === "string") return col;
    }
  }
  return data.columns[0];
}

/** Resolve the value (Y-axis / measure) field — prefers the numeric column */
export function resolveValueField(spec: WidgetSpec, data: QueryResult): string | undefined {
  if (spec.series?.[0]?.field) return spec.series[0].field;
  if (spec.y_axis?.field) return spec.y_axis.field;
  if (spec.data_mapping?.value) return spec.data_mapping.value;

  // Auto-detect: pick the first numeric column as value
  if (data.columns.length >= 2 && data.rows.length > 0) {
    const first = data.rows[0];
    for (const col of data.columns) {
      if (typeof first[col] === "number") return col;
    }
  }
  return data.columns[1];
}

// ---------------------------------------------------------------------------
// Data transformation
// ---------------------------------------------------------------------------

/** Convert query rows into { name, value } pairs for single-series charts */
export function toNameValuePairs(
  rows: QueryResult["rows"],
  labelField: string,
  valueField: string,
): { name: string; value: number }[] {
  return rows.map((row) => ({
    name: String(row[labelField] ?? ""),
    value: Number(row[valueField] ?? 0),
  }));
}

// ---------------------------------------------------------------------------
// Value formatting
// ---------------------------------------------------------------------------

/** Format a value for display — handles null, boolean, number, string, object */
export function formatValue(value: unknown): string {
  if (value == null) return "\u2014";
  if (typeof value === "boolean") return value ? "Yes" : "No";
  if (typeof value === "number") return value.toLocaleString();
  if (Array.isArray(value)) return value.map(formatValue).join(", ");
  if (typeof value === "object") {
    const obj = value as Record<string, unknown>;
    // PropertyValue wrapper: {type: "string", value: "..."}
    if ("type" in obj && "value" in obj) return formatValue(obj.value);
    if ("type" in obj && obj.type === "null") return "\u2014";
    // Node object with properties sub-object
    if ("properties" in obj && typeof obj.properties === "object" && obj.properties !== null) {
      const props = obj.properties as Record<string, unknown>;
      // Prefer name > label > title > id for display
      for (const key of ["name", "label", "title", "id"]) {
        if (key in props && props[key] != null) return formatValue(props[key]);
      }
    }
    // Plain object — prefer name > label > title > id
    for (const key of ["name", "label", "title", "id"]) {
      if (key in obj && obj[key] != null && typeof obj[key] !== "object") return String(obj[key]);
    }
    return JSON.stringify(value);
  }
  return String(value);
}
