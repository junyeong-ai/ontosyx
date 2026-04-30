"use client";

import { useTranslations } from "next-intl";

import type { NodeTypeDef } from "@/types/api";

import { SourceSampleMini } from "../source-sample-mini";

// ---------------------------------------------------------------------------
// SamplesFacet — `SourceSampleMini` wrapper. NodeType-only, since
// EdgeType has no source_lineage in the IR.
// ---------------------------------------------------------------------------

export function SamplesFacet({ node }: { node: NodeTypeDef }) {
  const t = useTranslations("workbench.entityFacets.samples");
  const table = node.source_lineage?.table;
  if (!table) {
    return (
      <p className="text-[11px] italic text-muted-foreground">
        {t("noSourceLineage")}
      </p>
    );
  }
  return <SourceSampleMini tableName={table} />;
}
