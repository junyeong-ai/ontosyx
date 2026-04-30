"use client";

import Link from "next/link";
import { useTranslations } from "next-intl";

import type { QualityGap } from "@/types/api";

import { GapsList } from "../quality-gaps";

// ---------------------------------------------------------------------------
// QualityFacet — surfaces this entity's quality gaps + a
// drill-through to the workspace-wide signals dashboard. Built as
// part of ADR-0054 so both the inspector and the domain-context
// page reach the same per-entity quality view; ADR-0057's
// severity-tiered overlay drops a chip on the canvas, this facet
// is where the operator lands when they click the chip.
// ---------------------------------------------------------------------------

export function QualityFacet({ gaps }: { gaps: QualityGap[] }) {
  const t = useTranslations("workbench.entityFacets.quality");

  if (gaps.length === 0) {
    return (
      <div className="flex flex-col gap-2">
        <p className="text-[11px] italic text-muted-foreground">
          {t("noGaps")}
        </p>
        <Link
          href="/settings/quality/signals"
          className="text-[11px] font-medium text-emerald-600 hover:underline dark:text-emerald-400"
        >
          {t("viewWorkspaceSignals")}
        </Link>
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-2">
      <GapsList gaps={gaps} />
      <Link
        href="/settings/quality/signals"
        className="text-[11px] font-medium text-emerald-600 hover:underline dark:text-emerald-400"
      >
        {t("viewWorkspaceSignals")}
      </Link>
    </div>
  );
}
