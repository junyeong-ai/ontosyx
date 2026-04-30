"use client";

import { useCallback } from "react";
import { useTranslations } from "next-intl";
import { toast } from "sonner";

import { useAppStore } from "@/lib/store";
import { NodeConstraintBuilder } from "@/components/ontology/node-constraint-builder";
import type { ConstraintDef, NodeTypeDef } from "@/types/api";

// ---------------------------------------------------------------------------
// ConstraintsFacet — UNIQUE / EXISTS / NODE KEY builder. NodeType
// only (the IR only carries constraints on nodes today).
// ---------------------------------------------------------------------------

export function ConstraintsFacet({ node }: { node: NodeTypeDef }) {
  const t = useTranslations("workbench.entityFacets.constraints");
  const applyCommand = useAppStore((s) => s.applyCommand);

  const handleAdd = useCallback(
    (constraint: ConstraintDef) => {
      applyCommand({
        op: "add_constraint",
        node_id: node.id,
        constraint,
      });
      toast.success(t("addedToast"));
    },
    [applyCommand, node.id, t],
  );

  const handleRemove = useCallback(
    (constraintId: string) => {
      applyCommand({
        op: "remove_constraint",
        node_id: node.id,
        constraint_id: constraintId,
      });
      toast.success(t("removedToast"));
    },
    [applyCommand, node.id, t],
  );

  return (
    <NodeConstraintBuilder
      node={node}
      onAdd={handleAdd}
      onRemove={handleRemove}
    />
  );
}
