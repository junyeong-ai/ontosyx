"use client";

import { useTranslations } from "next-intl";
import { useRouter, useSearchParams } from "next/navigation";

import { WorkbenchPageShell } from "@/components/workbench/workbench-page-shell";
import { CodeSystemsTab } from "./tabs/code-systems-tab";
import { ConceptMapsTab } from "./tabs/concept-maps-tab";
import { NotationPatternsTab } from "./tabs/notation-patterns-tab";
import { RulesTab } from "./tabs/rules-tab";
import { ValueSetsTab } from "./tabs/value-sets-tab";

const TAB_PARAM = "tab";
const ROUTE = "/vocabulary";

type VocabularyTab =
  | "code-systems"
  | "value-sets"
  | "concept-maps"
  | "notation-patterns"
  | "rules";

const TABS: ReadonlyArray<VocabularyTab> = [
  "code-systems",
  "value-sets",
  "concept-maps",
  "notation-patterns",
  "rules",
];

function isTab(v: string | null): v is VocabularyTab {
  return TABS.some((t) => t === v);
}

export function VocabularyWorkbench() {
  const t = useTranslations("workbench.vocabulary");
  const router = useRouter();
  const searchParams = useSearchParams();
  const urlTab = searchParams.get(TAB_PARAM);
  const tab: VocabularyTab = isTab(urlTab) ? urlTab : "code-systems";

  const setTab = (next: VocabularyTab) => {
    const params = new URLSearchParams(searchParams);
    params.set(TAB_PARAM, next);
    router.replace(`${ROUTE}?${params.toString()}`);
  };

  const tabItems = TABS.map((id) => ({ id, label: t(`tabs.${id}`) }));

  return (
    <WorkbenchPageShell
      title={t("heading")}
      tabs={tabItems}
      activeTab={tab}
      onTabChange={setTab}
      fillBleed
    >
      <div className="h-full min-h-0">
        {tab === "code-systems" && <CodeSystemsTab />}
        {tab === "value-sets" && <ValueSetsTab />}
        {tab === "concept-maps" && <ConceptMapsTab />}
        {tab === "notation-patterns" && <NotationPatternsTab />}
        {tab === "rules" && <RulesTab />}
      </div>
    </WorkbenchPageShell>
  );
}
