"use client";

import Link from "next/link";
import { useTranslations } from "next-intl";

import type { QualityGap } from "@/types/api";

import { QualityGapsList } from "../quality-gaps";

// ---------------------------------------------------------------------------
// QualityFacet — surfaces this entity's quality gaps + a
// drill-through to the workspace-wide signals dashboard. Both the
// inspector and the domain-context page reach this facet so they
// share a single per-entity quality view; the canvas severity chip
// links here when the operator drills into a flagged entity.
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
          className="text-[11px] font-medium text-brand-foreground hover:underline"
        >
          {t("viewWorkspaceSignals")}
        </Link>
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-2">
      <QualityGapsList gaps={gaps} />
      <Link
        href="/settings/quality/signals"
        className="text-[11px] font-medium text-brand-foreground hover:underline"
      >
        {t("viewWorkspaceSignals")}
      </Link>
    </div>
  );
}
