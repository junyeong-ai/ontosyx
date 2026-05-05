"use client";

import { useCallback } from "react";
import { useTranslations } from "next-intl";

import { useAppStore } from "@/lib/store";
import { defaultText } from "@/lib/locale/localize";
import { arr } from "@/lib/ir-collections";
import { GlossaryAnchorPicker } from "@/components/ontology/glossary-anchor-picker";
import type {
  EdgeTypeDef,
  NodeTypeDef,
  OntologyIR,
} from "@/types/api";
import type { GlossaryTermDef } from "@/lib/api/edit-ops";

import { InlineEdit } from "../inline-edit";
import { AiAssistButton, AiSuggestionList, useAiEdit } from "../ai-suggestions";

// ---------------------------------------------------------------------------
// DefinitionFacet — description + glossary anchors. Both NodeType
// and EdgeType variants share this surface; the difference is the
// op shape applyCommand sends, which we resolve via a single
// `kind` discriminator.
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

  const glossary: readonly GlossaryTermDef[] = arr(ontology.glossary);
  const anchors = arr(entity.glossary_anchors);

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

  const handleAnchorsChange = useCallback(
    (next: string[]) => {
      if (kind === "node") {
        applyCommand({
          op: "set_node_glossary_anchors",
          node_id: entity.id,
          anchors: next,
        });
      } else {
        applyCommand({
          op: "set_edge_glossary_anchors",
          edge_id: entity.id,
          anchors: next,
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
        <div className="mt-1 flex items-center gap-1">
          <InlineEdit
            value={defaultText(entity.description)}
            placeholder={t("descriptionPlaceholder")}
            onSave={handleUpdateDescription}
            className="flex-1 text-foreground-strong"
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
          {t("anchorsLabel")}
        </span>
        <p className="mt-0.5 text-2xs text-foreground-muted">
          {t("anchorsHint")}
        </p>
        <div className="mt-1.5">
          <GlossaryAnchorPicker
            value={anchors}
            glossary={glossary}
            onChange={handleAnchorsChange}
          />
        </div>
      </div>
    </div>
  );
}
