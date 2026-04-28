/**
 * Schema-level dependency graph client.
 *
 * Mirrors `ox_ontology::SchemaDependencyGraph` — a bidirectional
 * inverted index of every reference in the committed IR snapshot.
 * The FE pulls the whole graph once per ontology version and
 * resolves per-entity views client-side via [`dependentsOf`]
 * (inbound) and [`referencesOf`] (outbound).
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
 * client-side via [`dependentsOf`] / [`referencesOf`].
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
 * Resolve the inbound dependents of `target` — entities that
 * reference `target`. Returns an empty array when nothing depends
 * on the target. Linear in the bucket count which is bounded by
 * the entity count (typically a few hundred).
 */
export function dependentsOf(
  graph: SchemaDependencyGraph,
  target: SchemaEntityRef,
): readonly DependencyEdge[] {
  return lookup(graph.inbound, target);
}

/**
 * Resolve the outbound references of `source` — entities that
 * `source` references. Returns an empty array when the source
 * holds no outbound references.
 */
export function referencesOf(
  graph: SchemaDependencyGraph,
  source: SchemaEntityRef,
): readonly DependencyEdge[] {
  return lookup(graph.outbound, source);
}

function lookup(
  index: readonly DependencyBucket[],
  target: SchemaEntityRef,
): readonly DependencyEdge[] {
  const key = entityRefKey(target);
  for (const bucket of index) {
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
