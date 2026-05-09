"use client";

import { useCallback } from "react";
import { useTranslations } from "next-intl";

import { useAppStore } from "@/lib/store";
import { defaultText } from "@/lib/locale/localize";
import { arr } from "@/lib/ir-collections";
import type {
  EdgeTypeDef,
  NodeTypeDef,
  OntologyIR,
} from "@/types/api";

import { InlineEdit } from "../inline-edit";
import { AiAssistButton, AiSuggestionList, useAiEdit } from "../ai-suggestions";

// ---------------------------------------------------------------------------
// DefinitionFacet — description + concept realisation. Both NodeType
// and EdgeType variants share this surface; the difference is the
// op shape applyCommand sends, which we resolve via a single `kind`
// discriminator.
// ---------------------------------------------------------------------------

interface DefinitionFacetProps {
  ontology: OntologyIR;
  entity: NodeTypeDef | EdgeTypeDef;
  kind: "node" | "edge";
  /** When `false`, hides the AI-assist button on description.
   *  Both Inspector and Page expose AI assist; the prop is here
   *  so future read-only adapters can suppress it. */
  showAiAssist?: boolean;
}

export function DefinitionFacet({
  ontology,
  entity,
  kind,
  showAiAssist = true,
}: DefinitionFacetProps) {
  const t = useTranslations("workbench.entityFacets.definition");
  const applyCommand = useAppStore((s) => s.applyCommand);
  const ai = useAiEdit();

  const concept = arr(ontology.concepts).find((c) => c.id === entity.concept_id);
  const conceptTerm = concept
    ? arr(ontology.glossary).find((term) => term.id === concept.canonical_term_id)
    : undefined;

  const handleUpdateDescription = useCallback(
    (desc: string) => {
      const description = desc ? { default: desc } : undefined;
      if (kind === "node") {
        applyCommand({
          op: "update_node_description",
          node_id: entity.id,
          description,
        });
      } else {
        applyCommand({
          op: "update_edge_description",
          edge_id: entity.id,
          description,
        });
      }
    },
    [applyCommand, entity.id, kind],
  );

  const handleAiImproveDescription = useCallback(() => {
    const current = defaultText(entity.description);
    ai.requestEdit(
      `Improve the description for ${kind} '${entity.label}'${
        current ? ` (current: "${current}")` : ""
      }. Provide a clear, concise description.`,
    );
  }, [ai, entity.label, entity.description, kind]);

  return (
    <div className="space-y-3">
      <div>
        <span className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
          {t("descriptionLabel")}
        </span>
        <div className="mt-1 flex items-start gap-1">
          <InlineEdit
            value={defaultText(entity.description)}
            placeholder={t("descriptionPlaceholder")}
            onSave={handleUpdateDescription}
            className="flex-1 text-foreground-strong"
            multiline
          />
          {showAiAssist && ai.canEdit && (
            <AiAssistButton
              tooltip={t("aiAssistTooltip")}
              loading={ai.loading}
              onClick={handleAiImproveDescription}
            />
          )}
        </div>
        {ai.suggestions && (
          <AiSuggestionList
            commands={ai.suggestions.commands}
            explanation={ai.suggestions.explanation}
            onDismiss={ai.dismiss}
          />
        )}
      </div>
      <div>
        <span className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
          {t("conceptLabel")}
        </span>
        <p className="mt-0.5 text-2xs text-foreground-muted">
          {t("conceptHint")}
        </p>
        <div className="mt-1.5 rounded border border-divider bg-surface-inset px-2 py-1 text-2xs text-foreground-muted">
          {concept
            ? (defaultText(conceptTerm?.term) || concept.id)
            : t("noConcept")}
        </div>
      </div>
    </div>
  );
}
