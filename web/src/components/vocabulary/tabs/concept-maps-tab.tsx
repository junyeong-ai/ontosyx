"use client";

import { useTranslations } from "next-intl";

import { JsonEntityCrudPage } from "@/components/vocabulary/json-entity-crud-page";
import type { ConceptMapDef } from "@/types/ontology";

const CONCEPT_MAP_HINT = `{
  "id": "cm-status-iso",
  "name": "OrderStatus → ISO",
  "version": "1.0.0",
  "source_system_id": "cs-order-status",
  "target_system_id": "cs-iso-status",
  "mappings": [
    {
      "source_code": "PENDING",
      "target_code": "OPEN",
      "equivalence": "equivalent"
    }
  ]
}`;

export function ConceptMapsTab() {
  const t = useTranslations("settings.vocabulary.conceptMaps");
  return (
    <JsonEntityCrudPage<ConceptMapDef>
      schemaHint={CONCEPT_MAP_HINT}
      selectItems={(ir) => ir.concept_maps ?? []}
      itemId={(cm) => cm.id}
      buildCreateOp={(def) => ({ op: "create_concept_map", def })}
      buildUpdateOp={(id, def) => ({ op: "update_concept_map", id, def })}
      buildDeleteOp={(id) => ({ op: "delete_concept_map", id })}
      renderRow={(cm) => (
        <div>
          <div className="flex items-center gap-2 flex-wrap">
            <span className="font-mono text-sm font-medium text-foreground-strong">
              {cm.id}
            </span>
            <span className="text-xs text-foreground-muted">
              · {cm.name} · v{cm.version}
            </span>
          </div>
          <p className="mt-1 text-2xs text-foreground-subtle">
            {t("mappingSummary", {
              source: cm.source_system_id,
              target: cm.target_system_id,
              count: cm.mappings?.length ?? 0,
            })}
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
