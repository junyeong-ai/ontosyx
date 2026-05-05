"use client";

import { useTranslations } from "next-intl";

import { notationPatternSchema } from "@/components/forms/schemas/notation-pattern.schema";
import { MasterDetailEntityPage } from "@/components/vocabulary/master-detail-entity-page";
import {
  VocabularyUsageMap,
  collectNotationPatternUsages,
} from "@/components/vocabulary/usage-map";
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

export function NotationPatternsTab() {
  const t = useTranslations("settings.vocabulary.notationPatterns");
  const tCommon = useTranslations("common");
  return (
    <MasterDetailEntityPage<NotationPatternDef>
      schemaHint={NOTATION_PATTERN_HINT}
      schema={notationPatternSchema}
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
      renderUsage={(np, ir) => (
        <VocabularyUsageMap entries={collectNotationPatternUsages(ir, np.id)} />
      )}
      renderRow={(np) => (
        <div className="min-w-0 flex-1">
          <div className="truncate font-mono text-xs font-medium">
            {np.id}
          </div>
          <div className="mt-0.5 truncate text-2xs text-foreground-muted">
            {np.name} ·{" "}
            {t("componentCount", { count: np.components?.length ?? 0 })}
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
