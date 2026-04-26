"use client";

import { useTranslations } from "next-intl";

import { VocabularyListPage } from "@/components/settings/vocabulary/vocabulary-list-page";
import type { ValueSetDef } from "@/lib/api/edit-ops";

export default function ValueSetsAdminPage() {
  const t = useTranslations("settings.vocabulary.valueSets");
  return (
    <VocabularyListPage<ValueSetDef>
      title={t("pageTitle")}
      subtitle={t("pageSubtitle")}
      emptyTitle={t("empty.title")}
      emptyDescription={t("empty.description")}
      noOntologyMessage={t("noOntology")}
      confirmDeleteTitle={t("confirm.deleteTitle")}
      confirmDeleteDescription={(name) =>
        t("confirm.deleteDescription", { name })
      }
      deleteLabel={t("deleteButton")}
      deletedToast={(name) => t("toast.deleted", { name })}
      deleteFailedToast={(error) => t("toast.deleteFailed", { error })}
      deleteMessage={(name) => t("messages.deleted", { name })}
      selectItems={(ir) => (ir.value_sets as ValueSetDef[]) ?? []}
      itemId={(vs) => vs.id}
      itemName={(vs) => vs.name}
      buildDeleteOp={(id) => ({ op: "delete_value_set", id })}
      renderRow={(vs) => (
        <div>
          <div className="flex items-center gap-2 flex-wrap">
            <span className="font-mono text-sm font-medium text-zinc-900 dark:text-zinc-100">
              {vs.id}
            </span>
            <span className="text-xs text-zinc-500 dark:text-zinc-400">
              · {vs.name} · v{vs.version}
            </span>
          </div>
          <p className="mt-1 text-[10px] text-zinc-500 dark:text-zinc-500">
            {t("includeCount", {
              count: vs.composition?.length ?? 0,
            })}
          </p>
        </div>
      )}
    />
  );
}
