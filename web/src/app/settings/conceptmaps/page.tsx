"use client";

import { useTranslations } from "next-intl";

import { VocabularyListPage } from "@/components/settings/vocabulary/vocabulary-list-page";
import type { ConceptMapDef } from "@/lib/api/edit-ops";

export default function ConceptMapsAdminPage() {
  const t = useTranslations("settings.vocabulary.conceptMaps");
  return (
    <VocabularyListPage<ConceptMapDef>
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
      selectItems={(ir) => (ir.concept_maps as ConceptMapDef[]) ?? []}
      itemId={(cm) => cm.id}
      itemName={(cm) => cm.name}
      buildDeleteOp={(id) => ({ op: "delete_concept_map", id })}
      renderRow={(cm) => (
        <div>
          <div className="flex items-center gap-2 flex-wrap">
            <span className="font-mono text-sm font-medium text-zinc-900 dark:text-zinc-100">
              {cm.id}
            </span>
            <span className="text-xs text-zinc-500 dark:text-zinc-400">
              · {cm.name} · v{cm.version}
            </span>
          </div>
          <p className="mt-1 text-[10px] text-zinc-500 dark:text-zinc-500">
            {t("mappingSummary", {
              source: cm.source_system_id,
              target: cm.target_system_id,
              count: cm.mappings?.length ?? 0,
            })}
          </p>
        </div>
      )}
    />
  );
}
