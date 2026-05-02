"use client";

import { useTranslations } from "next-intl";

import { JsonEntityCrudPage } from "@/components/settings/vocabulary/json-entity-crud-page";
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
  return (
    <JsonEntityCrudPage<CodeSystemDef>
      schemaHint={CODE_SYSTEM_HINT}
      selectItems={(ir) =>
        ((ir as unknown as { code_systems?: CodeSystemDef[] }).code_systems ?? [])
      }
      itemId={(cs) => cs.id}
      buildCreateOp={(def) => ({ op: "create_code_system", def })}
      buildUpdateOp={(id, def) => ({ op: "update_code_system", id, def })}
      buildDeleteOp={(id) => ({ op: "delete_code_system", id })}
      renderRow={(cs) => (
        <div>
          <div className="flex flex-wrap items-center gap-2">
            <span className="font-mono text-sm font-medium text-foreground-strong dark:text-foreground">
              {cs.id}
            </span>
            <span className="text-xs text-muted-foreground dark:text-muted-foreground">
              · {cs.name} · v{cs.version} · {cs.kind}
            </span>
          </div>
          <p className="mt-1 text-2xs text-muted-foreground dark:text-muted-foreground">
            {t("codeCount", { count: cs.codes?.length ?? 0 })}
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
