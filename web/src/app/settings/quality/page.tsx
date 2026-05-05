"use client";

// Consolidated Quality page — `규칙` (SHACL-style validation rules),
// `지표` (6-tile observability dashboard), `비활성` (stale type
// proposals), `모호성` (NL→Cypher disambiguation queue) live as facets
// behind a single tabbed surface. All four are data-quality concerns;
// fragmenting them across separate sidebar items hid the relationships
// and inflated the governance group from 10 → 4 items.

import { useTranslations } from "next-intl";
import { useRouter, useSearchParams } from "next/navigation";

import { SettingsPageShell } from "@/components/layout/settings-page-shell";
import { TabBar } from "@/components/ui/tab-bar";
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

  const handleTabChange = (next: string) => {
    const url = next === "rules" ? "/settings/quality" : `/settings/quality?tab=${next}`;
    router.replace(url, { scroll: false });
  };

  return (
    <SettingsPageShell title={t("pageTitle")} subtitle={t("pageSubtitle")}>
      <div className="border-b border-divider">
        <TabBar
          tabs={[
            { id: "rules", label: t("tab.rules") },
            { id: "signals", label: t("tab.signals") },
            { id: "stale", label: t("tab.stale") },
            { id: "ambiguity", label: t("tab.ambiguity") },
          ]}
          activeTab={activeTab}
          onTabChange={handleTabChange}
        />
      </div>
      <div className="mt-4">
        {activeTab === "rules" && <RulesFacet />}
        {activeTab === "signals" && <SignalsFacet />}
        {activeTab === "stale" && <StaleFacet />}
        {activeTab === "ambiguity" && <AmbiguityFacet />}
      </div>
    </SettingsPageShell>
  );
}
