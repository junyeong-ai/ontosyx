"use client";

import { useTranslations } from "next-intl";

import { conceptMapSchema } from "@/components/forms/schemas/concept-map.schema";
import { MasterDetailEntityPage } from "@/components/vocabulary/master-detail-entity-page";
import {
  VocabularyUsageMap,
  collectConceptMapUsages,
} from "@/components/vocabulary/usage-map";
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
  const tCommon = useTranslations("common");
  return (
    <MasterDetailEntityPage<ConceptMapDef>
      schemaHint={CONCEPT_MAP_HINT}
      schema={conceptMapSchema}
      selectItems={(ir) => ir.concept_maps ?? []}
      itemId={(cm) => cm.id}
      buildCreateOp={(def) => ({ op: "create_concept_map", def })}
      buildUpdateOp={(id, def) => ({ op: "update_concept_map", id, def })}
      buildDeleteOp={(id) => ({ op: "delete_concept_map", id })}
      renderUsage={(cm, ir) => (
        <VocabularyUsageMap entries={collectConceptMapUsages(ir, cm.id)} />
      )}
      renderRow={(cm) => (
        <div className="min-w-0 flex-1">
          <div className="truncate font-mono text-xs font-medium">
            {cm.id}
          </div>
          <div className="mt-0.5 truncate text-2xs text-foreground-muted">
            {t("rowSummary", { name: cm.name, version: cm.version })}
            {" · "}
            {t("mappingCount", { count: cm.mappings?.length ?? 0 })}
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
