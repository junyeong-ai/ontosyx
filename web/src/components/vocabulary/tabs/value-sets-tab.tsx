"use client";

import { useTranslations } from "next-intl";

import { valueSetSchema } from "@/components/forms/schemas/value-set.schema";
import { MasterDetailEntityPage } from "@/components/vocabulary/master-detail-entity-page";
import {
  VocabularyUsageMap,
  collectValueSetUsages,
} from "@/components/vocabulary/usage-map";
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
  const tCommon = useTranslations("common");

  return (
    <MasterDetailEntityPage<ValueSetDef>
      schemaHint={VALUE_SET_HINT}
      schema={valueSetSchema}
      selectItems={(ir) =>
        ((ir as unknown as { value_sets?: ValueSetDef[] }).value_sets ?? [])
      }
      itemId={(vs) => vs.id}
      buildCreateOp={(def) => ({ op: "create_value_set", def })}
      buildUpdateOp={(id, def) => ({ op: "update_value_set", id, def })}
      buildDeleteOp={(id) => ({ op: "delete_value_set", id })}
      renderUsage={(vs, ir) => (
        <VocabularyUsageMap entries={collectValueSetUsages(ir, vs.id)} />
      )}
      renderRow={(vs) => (
        <div className="min-w-0 flex-1">
          <div className="truncate font-mono text-xs font-medium">
            {vs.id}
          </div>
          <div className="mt-0.5 truncate text-2xs text-foreground-muted">
            {t("rowSummary", { name: vs.name, version: vs.version })}
            {" · "}
            {t("includeCount", { count: vs.composition?.length ?? 0 })}
          </div>
        </div>
      )}
      labels={{
        title: t("pageTitle"),
        subtitle: t("pageSubtitle"),
        noOntology: t("noOntology"),
        listHeading: (count) => t("listHeading", { count }),
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
        loadErrorTitle: tCommon("loadError.title"),
        loadErrorDescription: tCommon("loadError.description"),
        retryLabel: tCommon("retry"),
      }}
    />
  );
}
