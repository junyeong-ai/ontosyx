"use client";

import { useTranslations } from "next-intl";
import { X } from "lucide-react";
import { toast } from "@/components/ui/toast";

import { useAppStore } from "@/lib/store";
import { Tooltip } from "@/components/ui/tooltip";
import { InlineObjectMappingEditor } from "@/components/ontology/inline-object-mapping-editor";
import { arr } from "@/lib/ir-collections";
import type { NodeTypeDef, OntologyIR } from "@/types/api";
import type { ObjectMappingDef } from "@/lib/api/edit-ops";

// ---------------------------------------------------------------------------
// MappingsFacet — single-mapping inline editor for a NodeType. The
// multi-mapping admin path stays at /mappings; this facet
// targets the common case (one mapping per node).
// ---------------------------------------------------------------------------

export function MappingsFacet({
  node,
  ontology,
}: {
  node: NodeTypeDef;
  ontology: OntologyIR;
}) {
  const t = useTranslations("workbench.entityFacets.mappings");
  const applyCommand = useAppStore((s) => s.applyCommand);
  const project = useAppStore((s) => s.activeOntologyDraft);

  const mappings = arr(ontology.object_mappings).filter(
    (m) => m.node_type_id === node.id,
  );
  const primary = mappings[0];
  const additional = mappings.length > 1 ? mappings.length - 1 : 0;

  const sourceColumns: readonly string[] | undefined = (() => {
    if (!primary?.relation || !project?.source_profile) return undefined;
    const profile = project.source_profile.table_profiles?.find(
      (tp) => tp.table_name === primary.relation,
    );
    return profile?.column_stats.map((c) => c.column_name);
  })();

  const handleCreate = () => {
    if (!project?.source_id) {
      toast.error(t("createBlockedNoSource"));
      return;
    }
    const mapping: ObjectMappingDef = {
      id: `om-${crypto.randomUUID()}`,
      node_type_id: node.id,
      source_id: project.source_id,
      relation: "",
      relation_kind: "table",
      property_mappings: [],
    };
    applyCommand({ op: "create_object_mapping", mapping });
    toast.success(t("createdToast"));
  };

  const handleUpdate = (mapping: ObjectMappingDef) => {
    applyCommand({ op: "update_object_mapping", id: mapping.id, mapping });
  };

  const handleDelete = (id: string) => {
    applyCommand({ op: "delete_object_mapping", id });
    toast.success(t("deletedToast"));
  };

  if (!primary) {
    return (
      <div className="space-y-2">
        <p className="text-2xs italic text-foreground-muted">
          {t("emptyState")}
        </p>
        <button
          type="button"
          onClick={handleCreate}
          disabled={!project?.source_id}
          className="inline-flex items-center gap-1 rounded border border-dashed border-divider px-2 py-1 text-2xs text-foreground-muted hover:border-concept-border hover:text-concept-foreground disabled:opacity-50"
        >
          {t("createAction")}
        </button>
      </div>
    );
  }

  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between gap-2">
        <span className="font-mono text-2xs text-foreground-muted">
          {t("primaryLabel", { id: primary.id })}
        </span>
        <Tooltip content={t("deleteTooltip")}>
          <button
            type="button"
            onClick={() => handleDelete(primary.id)}
            aria-label={t("deleteTooltip")}
            className="rounded p-0.5 text-foreground-muted hover:bg-danger-surface hover:text-danger-foreground"
          >
            <X className="h-3 w-3" />
          </button>
        </Tooltip>
      </div>
      <InlineObjectMappingEditor
        value={primary}
        properties={arr(node.properties)}
        availableColumns={sourceColumns}
        onChange={handleUpdate}
      />
      {additional > 0 && (
        <p className="rounded border border-warning-border bg-warning-surface px-3 py-2 text-2xs text-warning-foreground">
          {t("multiMappingHint", { count: additional })}
        </p>
      )}
    </div>
  );
}
