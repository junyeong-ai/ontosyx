"use client";

// Consolidated Quality page — `규칙` (SHACL-style validation rules),
// `지표` (6-tile observability dashboard), `비활성` (stale type
// proposals), `모호성` (NL→Cypher disambiguation queue) live as facets
// behind a single tabbed surface. All four are data-quality concerns;
// fragmenting them across separate sidebar items hid the relationships
// and inflated the governance group from 10 → 4 items.

import { useTranslations } from "next-intl";
import { useRouter, useSearchParams } from "next/navigation";

import { WorkbenchPageShell } from "@/components/workbench/workbench-page-shell";
import { useStaleProposals } from "@/hooks/api/use-quality";
import { useAmbiguities } from "@/hooks/api/use-ambiguities";
import { usePublishModeCount } from "@/hooks/use-publish-mode-count";
import { RulesFacet } from "./_facets/rules-facet";
import { SignalsFacet } from "./_facets/signals-facet";
import { StaleFacet } from "./_facets/stale-facet";
import { AmbiguityFacet } from "./_facets/ambiguity-facet";

const TABS = ["rules", "signals", "stale", "ambiguity"] as const;
type QualityTab = (typeof TABS)[number];

function isQualityTab(value: string | null): value is QualityTab {
  return value !== null && (TABS as readonly string[]).includes(value);
}

export default function QualityPage() {
  const t = useTranslations("settings.quality");
  const router = useRouter();
  const params = useSearchParams();

  const tabParam = params.get("tab");
  const activeTab: QualityTab = isQualityTab(tabParam) ? tabParam : "rules";

  // Publish a sidebar count that summarises all queues awaiting human
  // attention. Stale-concept proposals + ambiguity contexts both
  // surface row-level queues; the rules facet tracks runtime SHACL
  // failures which roll up via cron rather than a queue, so they
  // intentionally don't feed the badge.
  const staleQuery = useStaleProposals(false);
  const ambiguitiesQuery = useAmbiguities();
  const stalePending = staleQuery.data?.length ?? 0;
  const ambiguityPending =
    ambiguitiesQuery.data?.items.filter((a) => !a.active_resolution).length ??
    0;
  usePublishModeCount(
    "quality",
    stalePending + ambiguityPending,
    "warning",
  );

  const handleTabChange = (next: QualityTab) => {
    const url = next === "rules" ? "/quality" : `/quality?tab=${next}`;
    router.replace(url, { scroll: false });
  };

  return (
    <WorkbenchPageShell<QualityTab>
      title={t("pageTitle")}
      tabs={[
        { id: "rules", label: t("tab.rules") },
        { id: "signals", label: t("tab.signals") },
        { id: "stale", label: t("tab.stale") },
        { id: "ambiguity", label: t("tab.ambiguity") },
      ]}
      activeTab={activeTab}
      onTabChange={handleTabChange}
    >
      {activeTab === "rules" && <RulesFacet />}
      {activeTab === "signals" && <SignalsFacet />}
      {activeTab === "stale" && <StaleFacet />}
      {activeTab === "ambiguity" && <AmbiguityFacet />}
    </WorkbenchPageShell>
  );
}
