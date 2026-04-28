"use client";

import { useMemo } from "react";

import { useDependencyGraph } from "@/hooks/api/use-dependencies";
import {
  dependentsOf,
  type DependencyKind,
  type SchemaEntityRef,
} from "@/lib/api/dependencies";

interface DependentsBadgeProps {
  ontologyId: string | null | undefined;
  target: SchemaEntityRef;
}

/**
 * Inline badge that surfaces "N dependents" for the selected
 * entity, derived from the workspace's
 * [`SchemaDependencyGraph`]. Renders nothing while the graph is
 * loading or when the entity has no dependents — empty space is
 * the right indicator that there's nothing to break.
 *
 * Title attribute breaks the count down by [`DependencyKind`] so
 * the operator can hover to see "3 rules, 1 mapping, 2 edges"
 * without clicking through.
 */
export function DependentsBadge({ ontologyId, target }: DependentsBadgeProps) {
  const { data: graph } = useDependencyGraph(ontologyId);

  const summary = useMemo(() => {
    if (!graph) return null;
    const edges = dependentsOf(graph, target);
    if (edges.length === 0) return null;
    const byKind = new Map<DependencyKind, number>();
    for (const edge of edges) {
      byKind.set(edge.kind, (byKind.get(edge.kind) ?? 0) + 1);
    }
    const breakdown = [...byKind.entries()]
      .map(([kind, count]) => `${count} × ${humanize(kind)}`)
      .join(", ");
    return { count: edges.length, breakdown };
  }, [graph, target]);

  if (!summary) return null;

  return (
    <span
      className="rounded bg-violet-100 px-1.5 py-0.5 text-[9px] font-medium text-violet-700 dark:bg-violet-900/40 dark:text-violet-300"
      title={`Dependents: ${summary.breakdown}`}
    >
      {summary.count} dependent{summary.count === 1 ? "" : "s"}
    </span>
  );
}

/**
 * Map a [`DependencyKind`] to a short, human-readable phrase for the
 * tooltip breakdown. Kept inline — the catalogue would only have a
 * handful of entries and the kind set evolves slowly enough that a
 * `switch` is the lowest-friction surface.
 */
function humanize(kind: DependencyKind): string {
  switch (kind) {
    case "property_of":
      return "property";
    case "edge_source":
      return "edge source";
    case "edge_target":
      return "edge target";
    case "interface_implementation":
      return "interface impl";
    case "property_binding_ref":
      return "property binding";
    case "function_derivation":
      return "function derivation";
    case "unit_reference":
      return "unit";
    case "rule_constraint":
      return "rule";
    case "rule_activation":
      return "rule activation";
    case "rule_vocabulary":
      return "rule vocabulary";
    case "metric_scope":
      return "metric";
    case "action_target":
      return "action target";
    case "action_rule":
      return "action rule";
    case "enrichment_target":
      return "enrichment";
    case "object_mapping_target":
      return "object mapping";
    case "link_mapping_target":
      return "link mapping";
    case "property_mapping_target":
      return "property mapping";
    case "value_set_composition":
      return "value-set composition";
    case "concept_map_endpoint":
      return "concept-map endpoint";
    case "data_quality_target":
      return "data quality";
  }
}
