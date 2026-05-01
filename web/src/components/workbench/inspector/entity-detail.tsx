"use client";

import { useCallback, useMemo, useState } from "react";
import { useTranslations } from "next-intl";
import { HugeiconsIcon } from "@hugeicons/react";
import { Delete01Icon } from "@hugeicons/core-free-icons";
import { toast } from "sonner";

import { useAppStore } from "@/lib/store";
import { useConfirm } from "@/components/ui/confirm-dialog";
import { Tooltip } from "@/components/ui/tooltip";
import { TabBar } from "@/components/ui/tab-bar";
import { useEntityDependencies } from "@/hooks/api/use-entity-dependencies";
import type { SchemaEntityRef } from "@/lib/api/dependencies";
import type {
  OntologyIR,
  NodeTypeDef,
  EdgeTypeDef,
  QualityGap,
  ElementVerification,
} from "@/types/api";
import { defaultText } from "@/lib/locale/localize";
import { arr } from "@/lib/ir-collections";

import { DependentsBadge } from "./dependents-badge";
import { InlineEdit } from "./inline-edit";
import { DefinitionFacet } from "./facets/definition-facet";
import { PropertiesFacet } from "./facets/properties-facet";
import { SamplesFacet } from "./facets/samples-facet";
import { ConstraintsFacet } from "./facets/constraints-facet";
import { MappingsFacet } from "./facets/mappings-facet";
import { LineageFacet } from "./facets/lineage-facet";
import { QualityFacet } from "./facets/quality-facet";
import { ChangeLogFacet } from "./facets/change-log-facet";
import { Section } from "./shared";

// Re-exports kept for callers that imported the inspector body
// directly. The facets are the canonical surface — these stay so
// the test suite and any external consumers don't break.
export { InlineEdit } from "./inline-edit";
export { Section } from "./shared";
export { GapsList } from "./quality-gaps";

// ---------------------------------------------------------------------------
// Inspector tabs — five-pane navigation that mirrors the page-side
// CollapsibleSection set, minus a couple of always-relevant facets
// that ride directly under "Definition" because they're how an
// operator typically iterates on a type from the canvas.
//
// `definition` bundles Definition + Properties + Constraints +
// Mappings; the page splits them into independent accordion
// sections so a long-form context view doesn't bury Mappings under
// scroll. Each pane (in both adapters) calls the same facet
// component so the inspector and the page stay in lockstep.
// ---------------------------------------------------------------------------

type InspectorTab = "definition" | "sample" | "lineage" | "quality" | "changelog";

function useEntityRef(
  kind: "node_type" | "edge_type",
  id: string,
): SchemaEntityRef {
  return useMemo<SchemaEntityRef>(() => ({ kind, id }), [kind, id]);
}

// ---------------------------------------------------------------------------
// Verification badge — kept inline with the entity header.
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
// EntityHeader — shared chrome for NodeDetail + EdgeDetail. Owns
// label / description editing, deletion, and the verification
// badge. The same shape works for both kinds because the wire
// fields are aligned (label, description, glossary_anchors).
// ---------------------------------------------------------------------------

function EntityHeader({
  ontology,
  entity,
  kind,
  verifications,
  onVerify,
  onRename,
  onUpdateDescription,
  onDelete,
}: {
  ontology: OntologyIR;
  entity: NodeTypeDef | EdgeTypeDef;
  kind: "node" | "edge";
  verifications?: ElementVerification[];
  onVerify?: () => void;
  onRename: (label: string) => void;
  onUpdateDescription: (desc: string) => void;
  onDelete: () => void;
}) {
  const isEdge = kind === "edge";
  const edge = isEdge ? (entity as EdgeTypeDef) : null;
  const src = isEdge
    ? (arr(ontology.node_types).find((n) => n.id === edge?.source_node_id)
        ?.label ?? "?")
    : null;
  const tgt = isEdge
    ? (arr(ontology.node_types).find((n) => n.id === edge?.target_node_id)
        ?.label ?? "?")
    : null;

  return (
    <div className="border-b border-zinc-200 px-3 py-2 dark:border-zinc-800">
      <div className="flex items-center gap-2">
        <span
          className={
            isEdge
              ? "rounded bg-blue-100 px-1.5 py-0.5 text-[9px] font-bold uppercase text-blue-700 dark:bg-blue-900 dark:text-blue-400"
              : "rounded bg-emerald-100 px-1.5 py-0.5 text-[9px] font-bold uppercase text-emerald-700 dark:bg-emerald-900 dark:text-emerald-400"
          }
        >
          {isEdge ? "Edge" : "Node"}
        </span>
        <InlineEdit
          value={entity.label}
          onSave={onRename}
          className="font-semibold text-zinc-800 dark:text-zinc-200"
        />
        <DependentsBadge
          ontologyId={ontology.id}
          target={{
            kind: isEdge ? "edge_type" : "node_type",
            id: entity.id,
          }}
        />
        <Tooltip content={isEdge ? "Delete edge" : "Delete node"}>
          <button
            onClick={onDelete}
            aria-label={isEdge ? "Delete edge" : "Delete node"}
            className="ml-auto rounded p-1 text-zinc-300 hover:bg-red-50 hover:text-red-500 dark:hover:bg-red-950"
          >
            <HugeiconsIcon icon={Delete01Icon} className="h-3 w-3" size="100%" />
          </button>
        </Tooltip>
      </div>
      <div className="mt-1 flex items-center gap-1">
        <InlineEdit
          value={defaultText(entity.description)}
          placeholder="Add description..."
          onSave={onUpdateDescription}
          className="flex-1 text-muted-foreground"
        />
      </div>
      {isEdge && (
        <p className="mt-1 text-muted-foreground">
          {src} → {tgt}
          {edge?.cardinality && (
            <span className="ml-2">· Cardinality: {edge.cardinality}</span>
          )}
        </p>
      )}
      <div className="mt-1.5">
        <VerificationBadge
          verifications={verifications}
          elementId={entity.id}
          onVerify={onVerify}
        />
      </div>
    </div>
  );
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
  const [tab, setTab] = useState<InspectorTab>("definition");
  const confirm = useConfirm();
  const t = useTranslations("inspector.tabs");
  const ref = useEntityRef("node_type", node.id);
  const { inbound, outbound } = useEntityDependencies(ontology.id, ref);

  const handleRename = useCallback(
    (newLabel: string) => {
      applyCommand({ op: "rename_node", node_id: node.id, new_label: newLabel });
    },
    [applyCommand, node.id],
  );

  const handleUpdateDescription = useCallback(
    (desc: string) => {
      applyCommand({
        op: "update_node_description",
        node_id: node.id,
        description: desc ? { default: desc } : undefined,
      });
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

  const tabs = useMemo(
    () => [
      { id: "definition", label: t("definition") },
      ...(node.source_lineage?.table
        ? [{ id: "sample", label: t("sample") }]
        : []),
      {
        id: "lineage",
        label: t("lineage"),
        badge: outbound.length + inbound.length || undefined,
      },
      {
        id: "quality",
        label: t("quality"),
        badge: gaps.length || undefined,
      },
      { id: "changelog", label: t("changelog") },
    ],
    [t, node.source_lineage?.table, outbound.length, inbound.length, gaps.length],
  );

  return (
    <div className="flex h-full flex-col overflow-hidden text-xs">
      <EntityHeader
        ontology={ontology}
        entity={node}
        kind="node"
        verifications={verifications}
        onVerify={onVerify}
        onRename={handleRename}
        onUpdateDescription={handleUpdateDescription}
        onDelete={handleDeleteNode}
      />

      <div className="border-b border-zinc-200 dark:border-zinc-800">
        <TabBar
          tabs={tabs}
          activeTab={tab}
          onTabChange={(id) => setTab(id as InspectorTab)}
        />
      </div>

      <div className="min-h-0 flex-1 overflow-auto">
        {tab === "definition" && (
          <>
            <Section title={t("definitionSubsection.glossary")}>
              <div className="px-3 py-2">
                <DefinitionFacet
                  ontology={ontology}
                  entity={node}
                  kind="node"
                />
              </div>
            </Section>
            <Section title={`Properties (${arr(node.properties).length})`}>
              <div className="px-3 py-2">
                <PropertiesFacet
                  ontology={ontology}
                  entity={node}
                  kind="node"
                />
              </div>
            </Section>
            <Section
              title={`Constraints (${arr(node.constraints).length})`}
            >
              <div className="px-3 py-2">
                <ConstraintsFacet node={node} />
              </div>
            </Section>
            <Section title={t("definitionSubsection.mappings")}>
              <div className="px-3 py-2">
                <MappingsFacet node={node} ontology={ontology} />
              </div>
            </Section>
            <p className="mt-3 px-3 pb-2 text-[10px] text-muted-foreground">
              Tip: Press{" "}
              <kbd className="rounded bg-zinc-200 px-1 py-0.5 font-mono text-[9px] dark:bg-zinc-700">
                {"⌘"}K
              </kbd>{" "}
              to edit with AI
            </p>
          </>
        )}

        {tab === "sample" && (
          <div className="px-3 py-3">
            <SamplesFacet node={node} />
          </div>
        )}

        {tab === "lineage" && (
          <div className="px-3 py-3">
            <LineageFacet ontology={ontology} entityRef={ref} />
          </div>
        )}

        {tab === "quality" && (
          <div className="px-3 py-3">
            <QualityFacet gaps={gaps} />
          </div>
        )}

        {tab === "changelog" && (
          <div className="px-3 py-3">
            <ChangeLogFacet ontology={ontology} entity={node} kind="node" />
          </div>
        )}
      </div>
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
  const [tab, setTab] = useState<InspectorTab>("definition");
  const confirm = useConfirm();
  const t = useTranslations("inspector.tabs");
  const ref = useEntityRef("edge_type", edge.id);
  const { inbound, outbound } = useEntityDependencies(ontology.id, ref);

  const src =
    arr(ontology.node_types).find((n) => n.id === edge.source_node_id)?.label ??
    "?";
  const tgt =
    arr(ontology.node_types).find((n) => n.id === edge.target_node_id)?.label ??
    "?";

  const handleRename = useCallback(
    (newLabel: string) => {
      applyCommand({ op: "rename_edge", edge_id: edge.id, new_label: newLabel });
    },
    [applyCommand, edge.id],
  );

  const handleUpdateDescription = useCallback(
    (desc: string) => {
      applyCommand({
        op: "update_edge_description",
        edge_id: edge.id,
        description: desc ? { default: desc } : undefined,
      });
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

  const tabs = useMemo(
    () => [
      { id: "definition", label: t("definition") },
      {
        id: "lineage",
        label: t("lineage"),
        badge: outbound.length + inbound.length || undefined,
      },
      {
        id: "quality",
        label: t("quality"),
        badge: gaps.length || undefined,
      },
      { id: "changelog", label: t("changelog") },
    ],
    [t, outbound.length, inbound.length, gaps.length],
  );

  return (
    <div className="flex h-full flex-col overflow-hidden text-xs">
      <EntityHeader
        ontology={ontology}
        entity={edge}
        kind="edge"
        verifications={verifications}
        onVerify={onVerify}
        onRename={handleRename}
        onUpdateDescription={handleUpdateDescription}
        onDelete={handleDeleteEdge}
      />

      <div className="border-b border-zinc-200 dark:border-zinc-800">
        <TabBar
          tabs={tabs}
          activeTab={tab}
          onTabChange={(id) => setTab(id as InspectorTab)}
        />
      </div>

      <div className="min-h-0 flex-1 overflow-auto">
        {tab === "definition" && (
          <>
            <Section title={t("definitionSubsection.glossary")}>
              <div className="px-3 py-2">
                <DefinitionFacet
                  ontology={ontology}
                  entity={edge}
                  kind="edge"
                />
              </div>
            </Section>
            <Section title={`Properties (${arr(edge.properties).length})`}>
              <div className="px-3 py-2">
                <PropertiesFacet
                  ontology={ontology}
                  entity={edge}
                  kind="edge"
                />
              </div>
            </Section>
            <p className="mt-3 px-3 pb-2 text-[10px] text-muted-foreground">
              Tip: Press{" "}
              <kbd className="rounded bg-zinc-200 px-1 py-0.5 font-mono text-[9px] dark:bg-zinc-700">
                {"⌘"}K
              </kbd>{" "}
              to edit with AI
            </p>
          </>
        )}

        {tab === "lineage" && (
          <div className="px-3 py-3">
            <LineageFacet ontology={ontology} entityRef={ref} />
          </div>
        )}

        {tab === "quality" && (
          <div className="px-3 py-3">
            <QualityFacet gaps={gaps} />
          </div>
        )}

        {tab === "changelog" && (
          <div className="px-3 py-3">
            <ChangeLogFacet ontology={ontology} entity={edge} kind="edge" />
          </div>
        )}
      </div>
    </div>
  );
}
