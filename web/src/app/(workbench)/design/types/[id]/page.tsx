"use client";

import { use } from "react";

import { DomainContextPage } from "@/components/workbench/design/domain-context/domain-context-page";

/**
 * Entity-Centric Domain Context page — single surface that
 * consolidates every facet of one NodeType (definition, properties,
 * sample rows, constraints, mappings, lineage, change log) so a
 * non-technical modeller can shape one business concept without
 * juggling five admin pages.
 *
 * Lives under `/design/types/[id]` so deep-links (e.g. from search
 * results, audit logs, or external bookmarks) land on the same
 * surface a canvas double-click would.
 */
export default function DesignTypeDetailPage({
  params,
}: {
  params: Promise<{ id: string }>;
}) {
  const { id } = use(params);
  return <DomainContextPage nodeId={id} />;
}
