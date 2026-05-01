"use client";

import { useTranslations } from "next-intl";
import { useRouter, useSearchParams } from "next/navigation";

import { CodeSystemsTab } from "./code-systems-tab";
import { ConceptMapsTab } from "./concept-maps-tab";
import { NotationPatternsTab } from "./notation-patterns-tab";
import { RulesTab } from "./rules-tab";
import { ValueSetsTab } from "./value-sets-tab";

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

  return (
    <div className="flex h-full flex-col overflow-hidden bg-white dark:bg-zinc-950">
      <header className="flex items-center justify-between border-b border-zinc-200 px-4 py-3 dark:border-zinc-800">
        <div>
          <h1 className="text-sm font-semibold text-zinc-900 dark:text-zinc-100">
            {t("heading")}
          </h1>
          <p className="text-[11px] text-muted-foreground">{t("subtitle")}</p>
        </div>
      </header>
      <nav
        aria-label={t("tabsAria")}
        className="flex shrink-0 gap-1 border-b border-zinc-200 px-4 dark:border-zinc-800"
      >
        {TABS.map((k) => (
          <button
            key={k}
            type="button"
            onClick={() => setTab(k)}
            aria-pressed={tab === k}
            className={`relative px-3 py-2 text-xs font-medium ${
              tab === k
                ? "text-violet-700 dark:text-violet-400"
                : "text-muted-foreground hover:text-zinc-700 dark:hover:text-zinc-300"
            }`}
          >
            {t(`tabs.${k}`)}
            {tab === k && (
              <span className="absolute inset-x-0 -bottom-px h-0.5 bg-violet-500" />
            )}
          </button>
        ))}
      </nav>
      <div className="flex-1 overflow-auto">
        {tab === "code-systems" && <CodeSystemsTab />}
        {tab === "value-sets" && <ValueSetsTab />}
        {tab === "concept-maps" && <ConceptMapsTab />}
        {tab === "notation-patterns" && <NotationPatternsTab />}
        {tab === "rules" && <RulesTab />}
      </div>
    </div>
  );
}
