"use client";

import { useTranslations } from "next-intl";

import { JsonEntityCrudPage } from "@/components/vocabulary/json-entity-crud-page";
import type { ValueSetDef } from "@/lib/api/edit-ops";

const VALUE_SET_HINT = `{
  "id": "vs-order-status",
  "name": "OrderStatus",
  "version": "1.0.0",
  "composition": [
    {
      "system_id": "cs-order-status",
      "selector": { "kind": "all" },
      "mode": "include"
    }
  ]
}`;

export function ValueSetsTab() {
  const t = useTranslations("settings.vocabulary.valueSets");

  return (
    <JsonEntityCrudPage<ValueSetDef>
      schemaHint={VALUE_SET_HINT}
      selectItems={(ir) =>
        ((ir as unknown as { value_sets?: ValueSetDef[] }).value_sets ?? [])
      }
      itemId={(vs) => vs.id}
      buildCreateOp={(def) => ({ op: "create_value_set", def })}
      buildUpdateOp={(id, def) => ({ op: "update_value_set", id, def })}
      buildDeleteOp={(id) => ({ op: "delete_value_set", id })}
      renderRow={(vs) => (
        <div>
          <div className="flex items-center gap-2 flex-wrap">
            <span className="font-mono text-sm font-medium text-foreground-strong">
              {vs.id}
            </span>
            <span className="text-xs text-foreground-muted">
              · {vs.name} · v{vs.version}
            </span>
          </div>
          <p className="mt-1 text-2xs text-foreground-subtle">
            {t("includeCount", { count: vs.composition?.length ?? 0 })}
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
