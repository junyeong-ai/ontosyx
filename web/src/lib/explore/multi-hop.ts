// Phase 4.4 — BFS multi-hop expansion.
//
// `expandNode` returns the 1-hop neighbourhood. For depth > 1 we
// repeat on each newly-discovered neighbour while deduplicating by
// element_id and capping the total node count. The resulting list
// is what the explore layout shows as "neighbours" — it stays a
// flat `ExpandNeighbor[]` so the existing canvas / inspector code
// doesn't change shape.

import { expandNode, type ExpandNeighbor } from "@/lib/api/queries";

export interface MultiHopOptions {
  depth: 1 | 2 | 3;
  /** Cap on total returned neighbours. Protects the canvas from
   * a combinatorial blow-up on dense graphs. */
  maxNodes?: number;
  /** Per-expansion `limit` forwarded to `expandNode`. */
  perHopLimit?: number;
}

/**
 * BFS expansion starting at `rootId`. Returns the aggregated
 * neighbourhood (not including the root itself) up to the given
 * depth.
 *
 * Ordering is stable: 1-hop neighbours come first, 2-hop second,
 * 3-hop last — so the UI can colour / group by discovery depth if
 * it wants to later.
 */
export async function expandMultiHop(
  rootId: string,
  options: MultiHopOptions = { depth: 1 },
): Promise<ExpandNeighbor[]> {
  const { depth, maxNodes = 100, perHopLimit = 50 } = options;
  if (depth <= 1) {
    const r = await expandNode(rootId, perHopLimit);
    return r.neighbors.slice(0, maxNodes);
  }

  const seen = new Set<string>([rootId]);
  const out: ExpandNeighbor[] = [];
  let frontier: string[] = [rootId];

  for (let hop = 0; hop < depth; hop++) {
    if (frontier.length === 0) break;

    // Collect every expansion at this hop in parallel — the server
    // supports concurrent reads, and waiting serially would make
    // 3-hop feel sluggish on normal graphs.
    const expansions = await Promise.allSettled(
      frontier.map((id) => expandNode(id, perHopLimit)),
    );

    const nextFrontier: string[] = [];
    for (const result of expansions) {
      if (result.status !== "fulfilled") continue;
      for (const n of result.value.neighbors) {
        if (seen.has(n.element_id)) continue;
        seen.add(n.element_id);
        out.push(n);
        if (out.length >= maxNodes) {
          return out;
        }
        nextFrontier.push(n.element_id);
      }
    }
    frontier = nextFrontier;
  }
  return out;
}
