"use client";

import { useTranslations } from "next-intl";

import { JsonEntityCrudPage } from "@/components/settings/vocabulary/json-entity-crud-page";
import type { CodeSystemDef } from "@/lib/api/edit-ops";

// Code-system schema hint shown in the create dialog. Keeps the
// fixture inline so designers see a working JSON skeleton without
// hunting for sample data; the real persistence path goes through
// `/edits` ops the same way as inspector edits do.
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

/**
 * Code Systems tab — the registry of `CodeSystemDef`s the workspace
 * uses to anchor `ValueSetDef` bindings. Migrated from
 * `/settings/codesystems`; the underlying JsonEntityCrudPage is
 * unchanged, only the host is — settings sidebar was the wrong
 * place for vocabulary editing.
 */
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
