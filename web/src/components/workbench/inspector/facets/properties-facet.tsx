"use client";

import { useCallback, useState } from "react";
import { useTranslations } from "next-intl";
import { Plus } from "lucide-react";
import { toast } from "@/components/ui/toast";

import { useAppStore } from "@/lib/store";
import { Tooltip } from "@/components/ui/tooltip";
import { EmptyState } from "@/components/ui/empty-state";
import { arr } from "@/lib/ir-collections";
import type {
  EdgeTypeDef,
  NodeTypeDef,
  OntologyIR,
  PropertyPatch,
} from "@/types/api";

import { PropertyEditor, PropertyRow } from "../property-editor";
import { AiAssistButton, AiSuggestionList, useAiEdit } from "../ai-suggestions";

// ---------------------------------------------------------------------------
// PropertiesFacet — list, add, AI suggest. Both NodeType and
// EdgeType own a `properties` array, so the facet adapts via
// `kind` for the binding metadata + AI-prompt phrasing.
// ---------------------------------------------------------------------------

interface PropertiesFacetProps {
  ontology: OntologyIR;
  entity: NodeTypeDef | EdgeTypeDef;
  kind: "node" | "edge";
  showAiAssist?: boolean;
}

export function PropertiesFacet({
  ontology,
  entity,
  kind,
  showAiAssist = true,
}: PropertiesFacetProps) {
  const t = useTranslations("workbench.entityFacets.properties");
  const applyCommand = useAppStore((s) => s.applyCommand);
  const [adding, setAdding] = useState(false);
  const ai = useAiEdit();

  const handleDelete = useCallback(
    (propId: string, propName: string) => {
      applyCommand({
        op: "delete_property",
        owner_id: entity.id,
        property_id: propId,
      });
      toast.success(t("deletedToast", { name: propName }));
    },
    [applyCommand, entity.id, t],
  );

  const handleUpdate = useCallback(
    (propId: string, patch: PropertyPatch) => {
      applyCommand({
        op: "update_property",
        owner_id: entity.id,
        property_id: propId,
        patch,
      });
    },
    [applyCommand, entity.id],
  );

  const handleAiSuggest = useCallback(() => {
    ai.requestEdit(
      `Suggest additional properties for the '${entity.label}' ${kind} that would be useful based on the ontology context`,
    );
  }, [ai, entity.label, kind]);

  const properties = arr(entity.properties);

  return (
    <div className="space-y-1.5">
      <div className="flex items-center justify-end gap-1">
        {showAiAssist && ai.canEdit && (
          <AiAssistButton
            tooltip={t("aiSuggestTooltip")}
            loading={ai.loading}
            onClick={handleAiSuggest}
          />
        )}
        {!adding && (
          <Tooltip content={t("addAction")}>
            <button
              type="button"
              onClick={() => setAdding(true)}
              className="inline-flex items-center gap-1 rounded border border-dashed border-divider px-2 py-1 text-2xs text-foreground-muted hover:border-brand-border hover:text-brand-foreground"
            >
              <Plus className="h-3 w-3" />
              {t("addAction")}
            </button>
          </Tooltip>
        )}
      </div>
      {ai.suggestions && (
        <AiSuggestionList
          commands={ai.suggestions.commands}
          explanation={ai.suggestions.explanation}
          onDismiss={ai.dismiss}
        />
      )}
      {adding && (
        <PropertyEditor
          ownerId={entity.id}
          onClose={() => setAdding(false)}
        />
      )}
      {properties.length === 0 && !adding ? (
        <EmptyState variant="compact" title={t("emptyState")} />
      ) : (
        <ul className="divide-y divide-divider-soft rounded border border-divider-soft">
          {properties.map((prop) => (
            <li key={prop.id}>
              <PropertyRow
                prop={prop}
                onDelete={() => handleDelete(prop.id, prop.name)}
                onUpdate={(patch) => handleUpdate(prop.id, patch)}
                binding={
                  ontology.id
                    ? {
                        ontologyId: ontology.id,
                        expectedVersion: ontology.version.number,
                        ownerKind: kind,
                        ownerTypeId: entity.id,
                      }
                    : undefined
                }
              />
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
