import type { UiConfig } from "@/types/api";

/** Static fallback — used when the server config is unavailable. */
const DEFAULT_ELK_OPTIONS: Record<string, string> = {
  "elk.algorithm": "layered",
  "elk.direction": "RIGHT",
  "elk.spacing.nodeNode": "60",
  "elk.layered.spacing.nodeNodeBetweenLayers": "100",
  "elk.edgeRouting": "ORTHOGONAL",
  "elk.layered.considerModelOrder.strategy": "NODES_AND_EDGES",
  "elk.port.borderOffset": "0",
};

/**
 * Build ELK options from server UiConfig.
 * Falls back to static defaults for any missing values.
 */
export function buildElkOptions(config?: UiConfig): Record<string, string> {
  if (!config) return { ...DEFAULT_ELK_OPTIONS };

  return {
    ...DEFAULT_ELK_OPTIONS,
    "elk.direction": config.elk_direction,
    "elk.spacing.nodeNode": String(config.elk_node_spacing),
    "elk.layered.spacing.nodeNodeBetweenLayers": String(config.elk_layer_spacing),
    "elk.edgeRouting": config.elk_edge_routing,
  };
}

/**
 * Default ELK options for static usage (worker initialization, etc.).
 * Frozen to prevent accidental mutation (shares no reference with the mutable defaults).
 * For dynamic usage, prefer `buildElkOptions(config)`.
 */
export const ELK_OPTIONS: Readonly<Record<string, string>> = Object.freeze({
  ...DEFAULT_ELK_OPTIONS,
});
