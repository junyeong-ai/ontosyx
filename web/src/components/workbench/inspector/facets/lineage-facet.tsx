"use client";

import { useCallback, useMemo } from "react";
import { useTranslations } from "next-intl";

import { LineageTree } from "@/components/ontology/lineage-tree";
import { useEntityDependencies } from "@/hooks/api/use-entity-dependencies";
import type { SchemaEntityRef } from "@/lib/api/dependencies";
import { arr } from "@/lib/ir-collections";
import type { OntologyIR } from "@/types/api";

// ---------------------------------------------------------------------------
// LineageFacet — outbound + inbound dependency views for the
// entity. Renders both directions in stacked columns so the
// modeller sees "what this depends on" alongside "what depends on
// this" without flipping a tab. Inspector layouts may opt to hide
// one direction via `direction`.
// ---------------------------------------------------------------------------

interface LineageFacetProps {
  ontology: OntologyIR;
  entityRef: SchemaEntityRef;
  /** When omitted, both directions render side-by-side. Single
   *  values render only that direction (useful for the inspector's
   *  "Lineage" + "Dependents" split tabs). */
  direction?: "outbound" | "inbound";
}

export function LineageFacet({
  ontology,
  entityRef,
  direction,
}: LineageFacetProps) {
  const t = useTranslations("workbench.entityFacets.lineage");
  const { inbound, outbound, isLoading } = useEntityDependencies(
    ontology.id,
    entityRef,
  );

  const labelOf = useCallback(
    (target: SchemaEntityRef): string | null => {
      switch (target.kind) {
        case "node_type":
          return arr(ontology.node_types).find((n) => n.id === target.id)?.label ?? null;
        case "edge_type":
          return arr(ontology.edge_types).find((e) => e.id === target.id)?.label ?? null;
        default:
          return null;
      }
    },
    [ontology],
  );

  const showOutbound = direction === undefined || direction === "outbound";
  const showInbound = direction === undefined || direction === "inbound";

  const layout = useMemo(
    () =>
      direction === undefined
        ? "grid grid-cols-1 gap-3 lg:grid-cols-2"
        : "flex flex-col gap-1",
    [direction],
  );

  if (isLoading) {
    return (
      <p className="text-[11px] italic text-muted-foreground">
        {t("loading")}
      </p>
    );
  }

  return (
    <div className={layout}>
      {showOutbound && (
        <div className="space-y-1">
          {direction === undefined && (
            <h3 className="text-2xs font-semibold uppercase tracking-wider text-muted-foreground">
              {t("outboundHeader")}
            </h3>
          )}
          <LineageTree
            edges={outbound}
            direction="outbound"
            labelOf={labelOf}
          />
        </div>
      )}
      {showInbound && (
        <div className="space-y-1">
          {direction === undefined && (
            <h3 className="text-2xs font-semibold uppercase tracking-wider text-muted-foreground">
              {t("inboundHeader")}
            </h3>
          )}
          <LineageTree
            edges={inbound}
            direction="inbound"
            labelOf={labelOf}
          />
        </div>
      )}
    </div>
  );
}
