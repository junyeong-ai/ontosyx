import { unwrapPropertyValue } from "@/lib/api/normalization";
import type { BackendSearchNode } from "@/lib/api/queries";

// Re-export so explore components use a single import path
export { unwrapPropertyValue };

// ---------------------------------------------------------------------------
// Shared types for graph search results
// ---------------------------------------------------------------------------

export interface SearchResultNode {
  elementId: string;
  labels: string[];
  props: Record<string, unknown>;
}

/** Map backend search nodes (snake_case) to frontend SearchResultNode (camelCase) */
export function toSearchResultNodes(nodes: BackendSearchNode[]): SearchResultNode[] {
  return nodes.map((n) => ({
    elementId: n.element_id,
    labels: n.labels,
    props: n.props,
  }));
}

/** Extract a secondary subtitle from remaining properties (first 3 scalar fields) */
export function resolveSubtitle(props: Record<string, unknown>): string {
  const entries = Object.entries(props)
    .filter(([, v]) => {
      const raw = unwrapPropertyValue(v);
      return raw != null && typeof raw !== "object";
    })
    .slice(0, 3);
  return entries.map(([k, v]) => `${k}: ${unwrapPropertyValue(v)}`).join(" | ");
}

// ---------------------------------------------------------------------------
// Color palette — 50 perceptually distinct hex colors for NVL (no hsl())
// ---------------------------------------------------------------------------

export const LABEL_COLOR_PALETTE = [
  // Blues & Purples
  "#3b82f6", "#8b5cf6", "#6366f1", "#a855f7", "#7c3aed",
  "#2563eb", "#4f46e5", "#9333ea", "#6d28d9", "#4338ca",
  // Teals & Cyans
  "#06b6d4", "#14b8a6", "#0ea5e9", "#22d3ee", "#0891b2",
  "#0d9488", "#0284c7", "#0e7490", "#059669", "#047857",
  // Warm colors
  "#f59e0b", "#ef4444", "#ec4899", "#f97316", "#d946ef",
  "#e11d48", "#db2777", "#c026d3", "#ea580c", "#dc2626",
  // Greens & Limes
  "#84cc16", "#4ade80", "#a3e635", "#22c55e", "#16a34a",
  "#65a30d", "#15803d", "#10b981", "#34d399", "#2dd4bf",
  // Neutrals & Extras
  "#facc15", "#fb923c", "#e879f9", "#38bdf8", "#fbbf24",
  "#f472b6", "#818cf8", "#c084fc", "#67e8f9", "#86efac",
];

export const FOCUSED_NODE_COLOR = "#10b981"; // emerald

/** Deterministic hex color for a label string */
export function resolveNodeColor(label: string, isFocused: boolean): string {
  if (isFocused) return FOCUSED_NODE_COLOR;
  let hash = 0;
  for (let i = 0; i < label.length; i++) {
    hash = label.charCodeAt(i) + ((hash << 5) - hash);
  }
  return LABEL_COLOR_PALETTE[((hash % LABEL_COLOR_PALETTE.length) + LABEL_COLOR_PALETTE.length) % LABEL_COLOR_PALETTE.length];
}

// ---------------------------------------------------------------------------
// Display name resolution — generic, no domain-specific property names
// ---------------------------------------------------------------------------

/**
 * Priority keys for extracting a human-readable name from node properties.
 * Only universal naming conventions — no industry-specific keys.
 */
const DISPLAY_NAME_KEYS = [
  "name", "title", "label", "code", "email", "number", "id",
];

/**
 * Extract a human-readable display name from node properties.
 * Falls back to the first short string property, then the provided fallback.
 */
export function resolveDisplayName(
  props: Record<string, unknown>,
  fallback?: string,
): string {
  for (const key of DISPLAY_NAME_KEYS) {
    const raw = unwrapPropertyValue(props[key]);
    if (typeof raw === "string" && raw) return raw;
    if (typeof raw === "number") return String(raw);
  }
  // Fallback: first short string value among all properties
  for (const val of Object.values(props)) {
    const raw = unwrapPropertyValue(val);
    if (typeof raw === "string" && raw.length > 0 && raw.length < 50) return raw;
  }
  return fallback ?? "(unknown)";
}

/** Format a property value for display (unwrap + stringify) */
export function formatPropertyValue(value: unknown): string {
  const raw = unwrapPropertyValue(value);
  if (raw == null) return "null";
  if (typeof raw === "object") return JSON.stringify(raw);
  return String(raw);
}
