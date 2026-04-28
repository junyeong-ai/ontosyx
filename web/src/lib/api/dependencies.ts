/**
 * Schema-level dependency graph client.
 *
 * Mirrors `ox_ontology::SchemaDependencyGraph` — the inverted
 * reference index used by the editor's Inspector and the
 * standalone impact-analysis view to answer "what breaks if I
 * change this entity?"
 */

import type { components } from "@/types/api.generated";
import { request } from "./client";

export type SchemaEntityRef = components["schemas"]["SchemaEntityRef"];
export type DependencyEdge = components["schemas"]["DependencyEdge"];
export type DependencyKind = components["schemas"]["DependencyKind"];
export type DependencyBucket = components["schemas"]["DependencyBucket"];
export type SchemaDependencyGraph =
  components["schemas"]["SchemaDependencyGraph"];

/**
 * Fetch the full dependency graph for an ontology's current
 * version. The graph is small enough (entity-count × ~5 edges) to
 * round-trip in one response; the FE caches it and resolves
 * client-side via [`dependentsOf`].
 */
export async function getDependencyGraph(
  ontologyId: string,
): Promise<SchemaDependencyGraph> {
  const res = await request<{ data: SchemaDependencyGraph }>(
    `/ontologies/${encodeURIComponent(ontologyId)}/dependencies`,
  );
  return res.data;
}

/**
 * Resolve the dependents of a target [`SchemaEntityRef`] from a
 * pre-fetched [`SchemaDependencyGraph`]. Returns an empty array
 * when no entity references the target.
 *
 * Uses structural equality on the serialised entity reference so
 * lookup stays correct independent of BE enum-variant ordering —
 * FE callers can add new entity kinds without breaking adjacent
 * look-ups. Linear in the bucket count which is bounded by the
 * entity count (typically a few hundred).
 */
export function dependentsOf(
  graph: SchemaDependencyGraph,
  target: SchemaEntityRef,
): readonly DependencyEdge[] {
  const key = entityRefKey(target);
  for (const bucket of graph.buckets) {
    if (entityRefKey(bucket.target) === key) {
      return bucket.edges;
    }
  }
  return [];
}

/**
 * Stable string key for a [`SchemaEntityRef`] — usable as a React
 * key, Map key, or equality probe. The shape collapses every
 * variant's discriminator + ids into a single dotted token.
 */
export function entityRefKey(ref: SchemaEntityRef): string {
  switch (ref.kind) {
    case "property":
      return `${ref.kind}:${ref.owner}/${ref.id}`;
    case "coded_value":
      return `${ref.kind}:${ref.code_system}/${ref.id}`;
    default:
      return `${ref.kind}:${(ref as { id: string }).id}`;
  }
}
