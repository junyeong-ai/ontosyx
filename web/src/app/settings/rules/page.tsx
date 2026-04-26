"use client";

import { useTranslations } from "next-intl";

import { VocabularyListPage } from "@/components/settings/vocabulary/vocabulary-list-page";
import type { RuleDef } from "@/lib/api/edit-ops";

export default function RulesAdminPage() {
  const t = useTranslations("settings.vocabulary.rules");
  return (
    <VocabularyListPage<RuleDef>
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
      selectItems={(ir) => (ir.rules as RuleDef[]) ?? []}
      itemId={(r) => r.id}
      itemName={(r) => r.name}
      buildDeleteOp={(id) => ({ op: "delete_rule", id })}
      renderRow={(r) => (
        <div>
          <div className="flex items-center gap-2 flex-wrap">
            <span className="font-mono text-sm font-medium text-zinc-900 dark:text-zinc-100">
              {r.id}
            </span>
            <span className="text-xs text-zinc-500 dark:text-zinc-400">· {r.name}</span>
            {r.enforcement && (
              <span className="rounded bg-zinc-100 px-2 py-0.5 text-[10px] font-medium uppercase tracking-wider text-zinc-600 dark:bg-zinc-800 dark:text-zinc-300">
                {r.enforcement}
              </span>
            )}
            {r.severity && (
              <span
                className={`rounded px-2 py-0.5 text-[10px] font-medium uppercase tracking-wider ${
                  r.severity === "fail"
                    ? "bg-rose-100 text-rose-700 dark:bg-rose-950/30 dark:text-rose-300"
                    : r.severity === "warn"
                      ? "bg-amber-100 text-amber-700 dark:bg-amber-950/30 dark:text-amber-300"
                      : "bg-zinc-100 text-zinc-600 dark:bg-zinc-800 dark:text-zinc-300"
                }`}
              >
                {r.severity}
              </span>
            )}
          </div>
          <p className="mt-1 text-[10px] text-zinc-500 dark:text-zinc-500">
            {t("constraintCount", { count: r.constraints?.length ?? 0 })}
          </p>
        </div>
      )}
    />
  );
}
