"use client";

import { useTranslations } from "next-intl";

import { codeSystemSchema } from "@/components/forms/schemas/code-system.schema";
import { MasterDetailEntityPage } from "@/components/vocabulary/master-detail-entity-page";
import {
  VocabularyUsageMap,
  collectCodeSystemUsages,
} from "@/components/vocabulary/usage-map";
import type { CodeSystemDef } from "@/lib/api/edit-ops";

const CODE_SYSTEM_HINT = `{
  "id": "cs-order-status",
  "name": "OrderStatus",
  "version": "1.0.0",
  "kind": "internal",
  "codes": [
    { "id": "cv-pending", "code": "PENDING" },
    { "id": "cv-paid",    "code": "PAID" },
    { "id": "cv-shipped", "code": "SHIPPED" }
  ]
}`;

export function CodeSystemsTab() {
  const t = useTranslations("settings.vocabulary.codeSystems");
  const tCommon = useTranslations("common");
  return (
    <MasterDetailEntityPage<CodeSystemDef>
      schemaHint={CODE_SYSTEM_HINT}
      schema={codeSystemSchema}
      selectItems={(ir) =>
        ((ir as unknown as { code_systems?: CodeSystemDef[] }).code_systems ?? [])
      }
      itemId={(cs) => cs.id}
      buildCreateOp={(def) => ({ op: "create_code_system", def })}
      buildUpdateOp={(id, def) => ({ op: "update_code_system", id, def })}
      buildDeleteOp={(id) => ({ op: "delete_code_system", id })}
      renderUsage={(cs, ir) => (
        <VocabularyUsageMap entries={collectCodeSystemUsages(ir, cs.id)} />
      )}
      renderRow={(cs) => (
        <div className="min-w-0 flex-1">
          <div className="truncate font-mono text-xs font-medium">
            {cs.id}
          </div>
          <div className="mt-0.5 truncate text-2xs text-foreground-muted">
            {t("rowSummary", { name: cs.name, version: cs.version })}
            {" · "}
            {t("codeCount", { count: cs.codes?.length ?? 0 })}
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
