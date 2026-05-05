/**
 * Design tokens.
 *
 * Single source of truth for the color palette + spacing scale the
 * app uses across Tailwind classes, React Flow canvas SVGs, chart
 * libraries, and static admin reports. Putting them in one module
 * means every caller agrees on "what does `success` look like" and
 * gives the Quality Signals dashboard + `ResponseBasis` panel a
 * palette to pick from without re-declaring hex literals inline.
 */

/**
 * Tailwind-compatible color tokens. Values are the Tailwind palette
 * names — consumers pass them into `className={cn(...)}` expressions
 * directly.
 */
export const COLOR_TOKENS = {
  danger: "rose",
  warning: "amber",
  info: "violet",
  neutralInfo: "sky",
  success: "emerald",
  neutral: "zinc",
} as const;

export type ColorIntent = keyof typeof COLOR_TOKENS;

/**
 * Hex values for surfaces that don't parse Tailwind names — the
 * React Flow canvas renders SVG with literal hex strings. These are
 * the same colors as the Tailwind `-500` step for each intent, so
 * hovering over a node looks identical across canvas + chart.
 */
export const COLOR_HEX = {
  danger: "#f43f5e",
  warning: "#f59e0b",
  info: "#8b5cf6",
  neutralInfo: "#0ea5e9",
  success: "#10b981",
  neutral: "#71717a",
} as const satisfies Record<ColorIntent, string>;

/**
 * SHACL validator kinds → color intent. Used by the Quality
 * Signals dashboard's failure-distribution bar chart and the
 * `ResponseBasis` panel's diagnostic badges.
 */
export const SHACL_KIND_COLOR: Record<string, ColorIntent> = {
  cardinality: "danger",
  measure_group_by: "warning",
  missing_coded_value: "info",
  mandatory_missing: "neutralInfo",
  temporal_grain: "success",
  other: "neutral",
};

/**
 * Spacing scale — the app pins cell + row padding at `py-3 pe-6` for
 * settings tables (see `web/CLAUDE.md`). Exported here so components
 * reading "how wide is a settings column?" have an authoritative
 * answer without grepping for literal `py-3 pe-6` strings.
 */
export const SETTINGS_TABLE = {
  cellPaddingY: "py-3",
  cellPaddingX: "pe-6",
  minWidth7Cols: "min-w-[900px]",
  minWidth9Cols: "min-w-[1100px]",
} as const;

/**
 * Focus ring token. Used via `className="focus-visible:ring-2
 * focus-visible:ring-[token]"`. Centralising it means a theme-wide
 * change (e.g. contrast audit bumps the ring to `emerald-600`)
 * stays a one-line edit.
 */
export const FOCUS_RING = "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand-foreground focus-visible:ring-offset-2 focus-visible:ring-offset-white950";
