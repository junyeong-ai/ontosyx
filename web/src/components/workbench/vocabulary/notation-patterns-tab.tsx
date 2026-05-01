"use client";

import { useTranslations } from "next-intl";

import { JsonEntityCrudPage } from "@/components/settings/vocabulary/json-entity-crud-page";
import type { NotationPatternDef } from "@/lib/api/edit-ops";

const NOTATION_PATTERN_HINT = `{
  "id": "np-campaign-code",
  "name": "CampaignCode",
  "template": "{{campaign}}_{{year}}_{{seq}}",
  "separator": "_",
  "components": [
    { "name": "campaign", "kind": { "kind": "literal", "values": ["SPRING", "SUMMER"] } },
    { "name": "year",     "kind": { "kind": "year" } },
    { "name": "seq",      "kind": { "kind": "sequence", "width": 3 } }
  ]
}`;

/**
 * Notation Patterns tab — template-based identifier patterns the
 * workspace uses for naming-convention validation. Migrated from
 * /settings/notation-patterns; underlying JsonEntityCrudPage
 * unchanged, only the host.
 */
export function NotationPatternsTab() {
  const t = useTranslations("settings.vocabulary.notationPatterns");
  return (
    <JsonEntityCrudPage<NotationPatternDef>
      schemaHint={NOTATION_PATTERN_HINT}
      selectItems={(ir) =>
        ((ir as unknown as { notation_patterns?: NotationPatternDef[] })
          .notation_patterns ?? [])
      }
      itemId={(np) => np.id}
      buildCreateOp={(def) => ({ op: "create_notation_pattern", def })}
      buildUpdateOp={(id, def) => ({
        op: "update_notation_pattern",
        id,
        def,
      })}
      buildDeleteOp={(id) => ({ op: "delete_notation_pattern", id })}
      renderRow={(np) => (
        <div>
          <div className="flex items-center gap-2 flex-wrap">
            <span className="font-mono text-sm font-medium text-zinc-900 dark:text-zinc-100">
              {np.id}
            </span>
            <span className="text-xs text-zinc-500 dark:text-zinc-400">
              · {np.name}
            </span>
            {np.template && (
              <span className="rounded bg-zinc-100 px-2 py-0.5 font-mono text-[10px] text-zinc-600 dark:bg-zinc-800 dark:text-zinc-300">
                {np.template}
              </span>
            )}
          </div>
          <p className="mt-1 text-[10px] text-zinc-500 dark:text-zinc-500">
            {t("componentCount", { count: np.components?.length ?? 0 })}
          </p>
        </div>
      )}
      labels={{
        title: t("pageTitle"),
        subtitle: t("pageSubtitle"),
        noOntology: t("noOntology"),
        createButton: t("createButton"),
        editButton: t("editButton"),
        deleteButton: t("deleteButton"),
        emptyTitle: t("empty.title"),
        emptyDescription: t("empty.description"),
        confirmDeleteTitle: t("confirm.deleteTitle"),
        confirmDeleteDescription: (name) =>
          t("confirm.deleteDescription", { name }),
        createdToast: t("toast.created"),
        createFailedToast: (error) => t("toast.createFailed", { error }),
        updatedToast: t("toast.updated"),
        updateFailedToast: (error) => t("toast.updateFailed", { error }),
        deletedToast: t("toast.deleted"),
        deleteFailedToast: (error) => t("toast.deleteFailed", { error }),
        createdMessage: (name) => t("messages.created", { name }),
        updatedMessage: (name) => t("messages.updated", { name }),
        deletedMessage: (name) => t("messages.deleted", { name }),
        createDialogTitle: t("createDialog.title"),
        createDialogDescription: t("createDialog.description"),
        jsonLabel: t("jsonLabel"),
        submitCreate: t("form.submitCreate"),
        submitUpdate: t("form.submitUpdate"),
        cancel: t("form.cancel"),
        errorEmpty: t("error.empty"),
        errorInvalidJsonTemplate: (message) =>
          t("error.invalidJson", { message }),
      }}
    />
  );
}
