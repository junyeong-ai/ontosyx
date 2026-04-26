"use client";

import { useTranslations } from "next-intl";

import { VocabularyListPage } from "@/components/settings/vocabulary/vocabulary-list-page";
import type { CodeSystemDef } from "@/lib/api/edit-ops";

export default function CodeSystemsAdminPage() {
  const t = useTranslations("settings.vocabulary.codeSystems");
  return (
    <VocabularyListPage<CodeSystemDef>
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
      selectItems={(ir) => (ir.code_systems as CodeSystemDef[]) ?? []}
      itemId={(cs) => cs.id}
      itemName={(cs) => cs.name}
      buildDeleteOp={(id) => ({ op: "delete_code_system", id })}
      renderRow={(cs) => (
        <div>
          <div className="flex items-center gap-2 flex-wrap">
            <span className="font-mono text-sm font-medium text-zinc-900 dark:text-zinc-100">
              {cs.id}
            </span>
            <span className="text-xs text-zinc-500 dark:text-zinc-400">
              · {cs.name} · v{cs.version} · {cs.kind}
            </span>
          </div>
          <p className="mt-1 text-[10px] text-zinc-500 dark:text-zinc-500">
            {t("codeCount", { count: cs.codes?.length ?? 0 })}
          </p>
        </div>
      )}
    />
  );
}
