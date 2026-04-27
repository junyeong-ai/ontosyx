"use client";

import { useCallback, useState } from "react";
import { useAppStore } from "@/lib/store";
import { HugeiconsIcon } from "@hugeicons/react";
import {
  PlusSignIcon,
  Delete01Icon,
} from "@hugeicons/core-free-icons";
import { toast } from "sonner";
import { useConfirm } from "@/components/ui/confirm-dialog";
import { Tooltip } from "@/components/ui/tooltip";
import type {
  OntologyIR,
  NodeTypeDef,
  EdgeTypeDef,
  PropertyPatch,
  QualityGap,
  ElementVerification,
} from "@/types/api";
import { defaultText } from "@/lib/locale/localize";
import { InlineEdit } from "./inline-edit";
import { useAiEdit, AiSuggestionList, AiAssistButton } from "./ai-suggestions";
import { AddPropertyForm, PropertyRow } from "./property-editor";
import { Section, formatConstraint } from "./shared";
import { GapsList } from "./quality-gaps";
import { arr } from "@/lib/ir-collections";

// Re-export for external consumers
export { InlineEdit } from "./inline-edit";
export { Section } from "./shared";
export { GapsList } from "./quality-gaps";

// ---------------------------------------------------------------------------
// Verification badge
// ---------------------------------------------------------------------------

function VerificationBadge({
  verifications,
  elementId,
  onVerify,
}: {
  verifications?: ElementVerification[];
  elementId: string;
  onVerify?: () => void;
}) {
  const active = verifications?.find(
    (v) => v.element_id === elementId && !v.invalidated_at,
  );

  if (active) {
    return (
      <div className="flex items-center gap-1.5 rounded bg-emerald-50 px-2 py-1 text-[10px] text-emerald-700 dark:bg-emerald-950/40 dark:text-emerald-400">
        <span className="h-1.5 w-1.5 rounded-full bg-emerald-500" />
        <span>Verified by {active.verified_by_name ?? active.verified_by}</span>
      </div>
    );
  }

  if (onVerify) {
    return (
      <button
        onClick={onVerify}
        className="rounded border border-zinc-200 px-2 py-0.5 text-[10px] text-muted-foreground hover:bg-zinc-50 dark:border-zinc-700 dark:hover:bg-zinc-800"
      >
        Verify
      </button>
    );
  }

  return null;
}

// ---------------------------------------------------------------------------
// Node detail (editable)
// ---------------------------------------------------------------------------

export function NodeDetail({
  node,
  ontology,
  gaps,
  verifications,
  onVerify,
}: {
  node: NodeTypeDef;
  ontology: OntologyIR;
  gaps: QualityGap[];
  verifications?: ElementVerification[];
  onVerify?: () => void;
}) {
  const applyCommand = useAppStore((s) => s.applyCommand);
  const clearSelection = useAppStore((s) => s.clearSelection);
  const [addingProp, setAddingProp] = useState(false);
  const confirm = useConfirm();
  const { canEdit, loading: propsLoading, suggestions, requestEdit, dismiss } = useAiEdit();
  const { loading: descLoading, suggestions: descSuggestions, requestEdit: requestDescEdit, dismiss: dismissDesc } = useAiEdit();
  const anyAiLoading = propsLoading || descLoading;

  const connectedEdges = arr(ontology.edge_types).filter(
    (e) => e.source_node_id === node.id || e.target_node_id === node.id,
  );

  const handleRename = useCallback(
    (newLabel: string) => {
      applyCommand({ op: "rename_node", node_id: node.id, new_label: newLabel });
    },
    [applyCommand, node.id],
  );

  const handleUpdateDescription = useCallback(
    (desc: string) => {
      applyCommand({ op: "update_node_description", node_id: node.id, description: desc ? { default: desc } : undefined });
    },
    [applyCommand, node.id],
  );

  const handleDeleteNode = useCallback(async () => {
    const ok = await confirm({
      title: "Delete Node",
      description: `Delete "${node.label}" and all connected edges? This action cannot be undone.`,
      confirmLabel: "Delete",
      variant: "danger",
    });
    if (!ok) return;
    applyCommand({ op: "delete_node", node_id: node.id });
    clearSelection();
    toast.success(`Node "${node.label}" deleted`);
  }, [applyCommand, confirm, node.id, node.label, clearSelection]);

  const handleDeleteProperty = useCallback(
    (propId: string, propName: string) => {
      applyCommand({ op: "delete_property", owner_id: node.id, property_id: propId });
      toast.success(`Property "${propName}" deleted`);
    },
    [applyCommand, node.id],
  );

  const handleUpdateProperty = useCallback(
    (propId: string, patch: PropertyPatch) => {
      applyCommand({ op: "update_property", owner_id: node.id, property_id: propId, patch });
    },
    [applyCommand, node.id],
  );

  const handleRemoveConstraint = useCallback(
    (constraintId: string) => {
      applyCommand({ op: "remove_constraint", node_id: node.id, constraint_id: constraintId });
      toast.success("Constraint removed");
    },
    [applyCommand, node.id],
  );

  const handleAiSuggestProperties = useCallback(() => {
    requestEdit(
      `Suggest additional properties for the '${node.label}' node that would be useful based on the ontology context`,
    );
  }, [requestEdit, node.label]);

  const handleAiImproveDescription = useCallback(() => {
    requestDescEdit(
      `Improve the description for node '${node.label}'${node.description ? ` (current: "${node.description}")` : ""}. Provide a clear, concise description.`,
    );
  }, [requestDescEdit, node.label, node.description]);

  return (
    <div className="flex h-full flex-col overflow-auto text-xs">
      {/* Header */}
      <div className="border-b border-zinc-200 px-3 py-2 dark:border-zinc-800">
        <div className="flex items-center gap-2">
          <span className="rounded bg-emerald-100 px-1.5 py-0.5 text-[9px] font-bold uppercase text-emerald-700 dark:bg-emerald-900 dark:text-emerald-400">
            Node
          </span>
          <InlineEdit
            value={node.label}
            onSave={handleRename}
            className="font-semibold text-zinc-800 dark:text-zinc-200"
          />
          <Tooltip content="Delete node">
            <button
              onClick={handleDeleteNode}
              aria-label="Delete node"
              className="ml-auto rounded p-1 text-zinc-300 hover:bg-red-50 hover:text-red-500 dark:hover:bg-red-950"
            >
              <HugeiconsIcon icon={Delete01Icon} className="h-3 w-3" size="100%" />
            </button>
          </Tooltip>
        </div>
        <div className="mt-1 flex items-center gap-1">
          <InlineEdit
            value={defaultText(node.description)}
            placeholder="Add description..."
            onSave={handleUpdateDescription}
            className="flex-1 text-muted-foreground"
          />
          {canEdit && (
            <AiAssistButton
              tooltip="Improve description with AI"
              loading={anyAiLoading}
              onClick={handleAiImproveDescription}
            />
          )}
        </div>
        {descSuggestions && (
          <AiSuggestionList
            commands={descSuggestions.commands}
            explanation={descSuggestions.explanation}
            onDismiss={dismissDesc}
          />
        )}
        {node.source_lineage?.table && (
          <p className="mt-0.5 text-muted-foreground">Source: {node.source_lineage?.table}</p>
        )}
        <div className="mt-1.5">
          <VerificationBadge verifications={verifications} elementId={node.id} onVerify={onVerify} />
        </div>
      </div>

      {/* Properties */}
      <Section
        title={`Properties (${arr(node.properties).length})`}
        action={
          <>
            {canEdit && (
              <AiAssistButton
                tooltip="AI suggest properties"
                loading={anyAiLoading}
                onClick={handleAiSuggestProperties}
              />
            )}
            <Tooltip content="Add property">
              <button
                onClick={() => setAddingProp(true)}
                aria-label="Add property"
                className="rounded p-0.5 text-muted-foreground hover:bg-zinc-100 hover:text-zinc-600 dark:hover:bg-zinc-800"
              >
                <HugeiconsIcon icon={PlusSignIcon} className="h-3 w-3" size="100%" />
              </button>
            </Tooltip>
          </>
        }
      >
        {suggestions && (
          <AiSuggestionList
            commands={suggestions.commands}
            explanation={suggestions.explanation}
            onDismiss={dismiss}
          />
        )}
        {addingProp && (
          <AddPropertyForm ownerId={node.id} onClose={() => setAddingProp(false)} />
        )}
        {arr(node.properties).map((prop) => (
          <PropertyRow
            key={prop.id}
            prop={prop}
            onDelete={() => handleDeleteProperty(prop.id, prop.name)}
            onUpdate={(patch) => handleUpdateProperty(prop.id, patch)}
            binding={
              ontology.id
                ? {
                    ontologyId: ontology.id,
                    expectedVersion: ontology.version,
                    ownerKind: "node",
                    ownerTypeId: node.id,
                  }
                : undefined
            }
          />
        ))}
      </Section>

      {/* Constraints */}
      {node.constraints && arr(node.constraints).length > 0 && (
        <Section title={`Constraints (${arr(node.constraints).length})`}>
          {arr(node.constraints).map((cd) => (
            <div key={cd.id} className="group flex items-center justify-between px-3 py-1 text-zinc-600 dark:text-muted-foreground">
              <span>{formatConstraint(cd, node)}</span>
              <Tooltip content="Remove constraint">
                <button
                  onClick={() => handleRemoveConstraint(cd.id)}
                  aria-label="Remove constraint"
                  className="rounded p-0.5 text-zinc-300 opacity-0 transition-opacity hover:text-red-500 group-hover:opacity-100"
                >
                  <HugeiconsIcon icon={Delete01Icon} className="h-2.5 w-2.5" size="100%" />
                </button>
              </Tooltip>
            </div>
          ))}
        </Section>
      )}

      {/* Connected edges */}
      {connectedEdges.length > 0 && (
        <Section title={`Relationships (${connectedEdges.length})`}>
          {connectedEdges.map((edge) => {
            const src = arr(ontology.node_types).find((n) => n.id === edge.source_node_id)?.label ?? "?";
            const tgt = arr(ontology.node_types).find((n) => n.id === edge.target_node_id)?.label ?? "?";
            return (
              <div key={edge.id} className="px-3 py-1 text-zinc-600 dark:text-muted-foreground">
                {src} —[{edge.label}]→ {tgt}
              </div>
            );
          })}
        </Section>
      )}

      <GapsList gaps={gaps} />

      <p className="mt-3 px-3 pb-2 text-[10px] text-muted-foreground">
        Tip: Press <kbd className="rounded bg-zinc-200 px-1 py-0.5 font-mono text-[9px] dark:bg-zinc-700">{"\u2318"}K</kbd> to edit with AI
      </p>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Edge detail (editable)
// ---------------------------------------------------------------------------

export function EdgeDetail({
  edge,
  ontology,
  gaps,
  verifications,
  onVerify,
}: {
  edge: EdgeTypeDef;
  ontology: OntologyIR;
  gaps: QualityGap[];
  verifications?: ElementVerification[];
  onVerify?: () => void;
}) {
  const applyCommand = useAppStore((s) => s.applyCommand);
  const clearSelection = useAppStore((s) => s.clearSelection);
  const [addingProp, setAddingProp] = useState(false);
  const confirm = useConfirm();
  const { canEdit, loading: propsLoading, suggestions, requestEdit, dismiss } = useAiEdit();
  const { loading: descLoading, suggestions: descSuggestions, requestEdit: requestDescEdit, dismiss: dismissDesc } = useAiEdit();
  const anyAiLoading = propsLoading || descLoading;

  const src = arr(ontology.node_types).find((n) => n.id === edge.source_node_id)?.label ?? "?";
  const tgt = arr(ontology.node_types).find((n) => n.id === edge.target_node_id)?.label ?? "?";

  const handleRename = useCallback(
    (newLabel: string) => {
      applyCommand({ op: "rename_edge", edge_id: edge.id, new_label: newLabel });
    },
    [applyCommand, edge.id],
  );

  const handleUpdateDescription = useCallback(
    (desc: string) => {
      applyCommand({ op: "update_edge_description", edge_id: edge.id, description: desc ? { default: desc } : undefined });
    },
    [applyCommand, edge.id],
  );

  const handleDeleteEdge = useCallback(async () => {
    const ok = await confirm({
      title: "Delete Edge",
      description: `Delete edge "${edge.label}" (${src} → ${tgt})? This action cannot be undone.`,
      confirmLabel: "Delete",
      variant: "danger",
    });
    if (!ok) return;
    applyCommand({ op: "delete_edge", edge_id: edge.id });
    clearSelection();
    toast.success(`Edge "${edge.label}" deleted`);
  }, [applyCommand, confirm, edge.id, edge.label, clearSelection, src, tgt]);

  const handleDeleteProperty = useCallback(
    (propId: string, propName: string) => {
      applyCommand({ op: "delete_property", owner_id: edge.id, property_id: propId });
      toast.success(`Property "${propName}" deleted`);
    },
    [applyCommand, edge.id],
  );

  const handleUpdateProperty = useCallback(
    (propId: string, patch: PropertyPatch) => {
      applyCommand({ op: "update_property", owner_id: edge.id, property_id: propId, patch });
    },
    [applyCommand, edge.id],
  );

  const handleAiSuggestProperties = useCallback(() => {
    requestEdit(
      `Suggest additional properties for the '${edge.label}' edge (${src} -> ${tgt}) that would be useful based on the ontology context`,
    );
  }, [requestEdit, edge.label, src, tgt]);

  const handleAiImproveDescription = useCallback(() => {
    requestDescEdit(
      `Improve the description for edge '${edge.label}' (${src} -> ${tgt})${edge.description ? ` (current: "${edge.description}")` : ""}. Provide a clear, concise description.`,
    );
  }, [requestDescEdit, edge.label, edge.description, src, tgt]);

  return (
    <div className="flex h-full flex-col overflow-auto text-xs">
      {/* Header */}
      <div className="border-b border-zinc-200 px-3 py-2 dark:border-zinc-800">
        <div className="flex items-center gap-2">
          <span className="rounded bg-blue-100 px-1.5 py-0.5 text-[9px] font-bold uppercase text-blue-700 dark:bg-blue-900 dark:text-blue-400">
            Edge
          </span>
          <InlineEdit
            value={edge.label}
            onSave={handleRename}
            className="font-semibold text-zinc-800 dark:text-zinc-200"
          />
          <Tooltip content="Delete edge">
            <button
              onClick={handleDeleteEdge}
              aria-label="Delete edge"
              className="ml-auto rounded p-1 text-zinc-300 hover:bg-red-50 hover:text-red-500 dark:hover:bg-red-950"
            >
              <HugeiconsIcon icon={Delete01Icon} className="h-3 w-3" size="100%" />
            </button>
          </Tooltip>
        </div>
        <div className="mt-1 flex items-center gap-1">
          <InlineEdit
            value={defaultText(edge.description)}
            placeholder="Add description..."
            onSave={handleUpdateDescription}
            className="flex-1 text-muted-foreground"
          />
          {canEdit && (
            <AiAssistButton
              tooltip="Improve description with AI"
              loading={anyAiLoading}
              onClick={handleAiImproveDescription}
            />
          )}
        </div>
        {descSuggestions && (
          <AiSuggestionList
            commands={descSuggestions.commands}
            explanation={descSuggestions.explanation}
            onDismiss={dismissDesc}
          />
        )}
        <p className="mt-1 text-muted-foreground">
          {src} → {tgt}
        </p>
        {edge.cardinality && (
          <p className="text-muted-foreground">Cardinality: {edge.cardinality}</p>
        )}
        <div className="mt-1.5">
          <VerificationBadge verifications={verifications} elementId={edge.id} onVerify={onVerify} />
        </div>
      </div>

      {/* Properties */}
      <Section
        title={`Properties (${arr(edge.properties).length})`}
        action={
          <>
            {canEdit && (
              <AiAssistButton
                tooltip="AI suggest properties"
                loading={anyAiLoading}
                onClick={handleAiSuggestProperties}
              />
            )}
            <Tooltip content="Add property">
              <button
                onClick={() => setAddingProp(true)}
                aria-label="Add property"
                className="rounded p-0.5 text-muted-foreground hover:bg-zinc-100 hover:text-zinc-600 dark:hover:bg-zinc-800"
              >
                <HugeiconsIcon icon={PlusSignIcon} className="h-3 w-3" size="100%" />
              </button>
            </Tooltip>
          </>
        }
      >
        {suggestions && (
          <AiSuggestionList
            commands={suggestions.commands}
            explanation={suggestions.explanation}
            onDismiss={dismiss}
          />
        )}
        {addingProp && (
          <AddPropertyForm ownerId={edge.id} onClose={() => setAddingProp(false)} />
        )}
        {arr(edge.properties).map((prop) => (
          <PropertyRow
            key={prop.id}
            prop={prop}
            onDelete={() => handleDeleteProperty(prop.id, prop.name)}
            onUpdate={(patch) => handleUpdateProperty(prop.id, patch)}
            binding={
              ontology.id
                ? {
                    ontologyId: ontology.id,
                    expectedVersion: ontology.version,
                    ownerKind: "edge",
                    ownerTypeId: edge.id,
                  }
                : undefined
            }
          />
        ))}
      </Section>

      <GapsList gaps={gaps} />

      <p className="mt-3 px-3 pb-2 text-[10px] text-muted-foreground">
        Tip: Press <kbd className="rounded bg-zinc-200 px-1 py-0.5 font-mono text-[9px] dark:bg-zinc-700">{"\u2318"}K</kbd> to edit with AI
      </p>
    </div>
  );
}
