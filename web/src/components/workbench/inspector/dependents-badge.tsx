"use client";

import { useMemo } from "react";
import { useTranslations } from "next-intl";

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
  const t = useTranslations("inspector.dependents");
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
      .map(([kind, count]) => t("breakdownItem", { count, kind: t(`kind.${kind}`) }))
      .join(", ");
    return { count: edges.length, breakdown };
  }, [graph, target, t]);

  if (!summary) return null;

  return (
    <span
      className="rounded bg-concept-surface px-1.5 py-0.5 text-2xs font-medium text-concept-foreground"
      title={t("titleAttr", { breakdown: summary.breakdown })}
    >
      {t("count", { count: summary.count })}
    </span>
  );
}

