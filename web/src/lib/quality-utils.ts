"use client";

import type { QualityGap } from "@/types/api";
import { useAppStore } from "@/lib/store";

/**
 * Resolve a quality gap to the canvas entity it refers to.
 * Returns null for source-only gaps (source_table, source_column, source_foreign_key)
 * that have no corresponding node/edge on the canvas.
 */
export function getGapEntityId(
  gap: QualityGap,
): { type: "node" | "edge"; id: string } | null {
  const loc = gap.location;
  if (loc.ref_type === "node" || loc.ref_type === "node_property") {
    return { type: "node", id: loc.node_id };
  }
  if (loc.ref_type === "edge" || loc.ref_type === "edge_property") {
    return { type: "edge", id: loc.edge_id };
  }
  return null;
}

/**
 * Membership test for "does this gap touch the named entity?". The
 * inspector and the domain-context page both filter the workspace-
 * wide gap list down to a single entity; centralising the predicate
 * here keeps the two surfaces in lockstep.
 */
export function gapTouchesEntity(
  gap: QualityGap,
  entityKind: "node" | "edge",
  entityId: string,
): boolean {
  const target = getGapEntityId(gap);
  return target?.type === entityKind && target.id === entityId;
}

/**
 * Navigate to the entity referenced by a quality gap:
 * select it on the canvas and ensure the inspector panel is open.
 * Returns true if navigation occurred, false if the gap has no canvas anchor.
 */
export function navigateToGap(gap: QualityGap): boolean {
  const entity = getGapEntityId(gap);
  if (!entity) return false;

  const state = useAppStore.getState();
  if (entity.type === "node") {
    state.select({ type: "node", nodeId: entity.id });
  } else {
    state.select({ type: "edge", edgeId: entity.id });
  }
  if (!state.isInspectorOpen) {
    state.toggleInspector();
  }
  return true;
}
