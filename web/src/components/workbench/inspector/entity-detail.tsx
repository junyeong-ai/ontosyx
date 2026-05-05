"use client";

import { useCallback, useMemo } from "react";
import { useTranslations } from "next-intl";
import { HugeiconsIcon } from "@hugeicons/react";
import { Delete01Icon } from "@hugeicons/core-free-icons";
import { toast } from "@/components/ui/toast";

import { useAppStore } from "@/lib/store";
import { useConfirm } from "@/components/providers/confirm-provider";
import { Tooltip } from "@/components/ui/tooltip";
import { TabBar } from "@/components/ui/tab-bar";
import { LockIndicator } from "@/components/collab/lock-indicator";
import { useEntityLock } from "@/components/collab/use-entity-lock";
import { colorFor, selectPresence, useCollabStore } from "@/lib/collab";
import { cn } from "@/lib/cn";
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
import {
  inspectorFacetById,
  visibleInspectorFacets,
  type InspectorFacetContext,
  type InspectorFacetId,
} from "./facets/registry";

// Re-exports kept for callers that imported the inspector body
// directly. The facets are the canonical surface — these stay so
// the test suite and any external consumers don't break.
export { InlineEdit } from "./inline-edit";
export { Section } from "./shared";
export { QualityGapsList } from "./quality-gaps";

// ---------------------------------------------------------------------------
// Inspector tabs — facet registry drives the tab strip and the body
// switch. Adding a new tab is one entry in `INSPECTOR_FACETS`
// that ride directly under "Definition" because they're how an
// operator typically iterates on a type from the canvas.
//
// `definition` bundles Definition + Properties + Constraints +
// Mappings; the page splits them into independent accordion
// sections so a long-form context view doesn't bury Mappings under
// scroll. Each pane (in both adapters) calls the same facet
// component so the inspector and the page stay in lockstep.
// ---------------------------------------------------------------------------

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

  const t = useTranslations("inspector.entity");
  if (active) {
    return (
      <div className="flex items-center gap-1.5 rounded bg-brand-surface px-2 py-1 text-2xs text-brand-foreground">
        <span className="h-1.5 w-1.5 rounded-full bg-brand-solid" />
        <span>{t("verifiedBy", { name: active.verified_by_name ?? active.verified_by })}</span>
      </div>
    );
  }

  if (onVerify) {
    return (
      <button type="button"
        onClick={onVerify}
        className="rounded border border-divider px-2 py-0.5 text-2xs text-foreground-muted hover:bg-surface-raised"
      >
        {t("verify")}
      </button>
    );
  }

  return null;
}

// ---------------------------------------------------------------------------
// EntityHeader — shared chrome for EntityDetail + EdgeDetail. Owns
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
  const t = useTranslations("inspector.entity");
  const tAria = useTranslations("inspector.aria");
  const activeProject = useAppStore((s) => s.activeProject);
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
    <div className="border-b border-divider px-3 py-2">
      <div className="flex items-center gap-2">
        <span
          className={
            isEdge
              ? "rounded bg-info-surface px-1.5 py-0.5 text-2xs font-bold uppercase text-info-foreground"
              : "rounded bg-brand-surface-strong px-1.5 py-0.5 text-2xs font-bold uppercase text-brand-foreground-strong"
          }
        >
          {isEdge ? "Edge" : "Node"}
        </span>
        <InlineEdit
          value={entity.label}
          onSave={onRename}
          className="font-semibold text-foreground-strong"
        />
        <DependentsBadge
          ontologyId={ontology.id}
          target={{
            kind: isEdge ? "edge_type" : "node_type",
            id: entity.id,
          }}
        />
        <LockIndicator
          projectId={activeProject?.id}
          entityId={entity.id}
          className="ms-auto"
        />
        <Tooltip content={isEdge ? tAria("deleteEdge") : tAria("deleteNode")}>
          <button type="button"
            onClick={onDelete}
            aria-label={isEdge ? tAria("deleteEdge") : tAria("deleteNode")}
            className="rounded p-1 text-foreground-muted hover:bg-danger-surface hover:text-danger-foreground"
          >
            <HugeiconsIcon icon={Delete01Icon} className="h-3 w-3" size="100%" />
          </button>
        </Tooltip>
      </div>
      <div className="mt-1 flex items-center gap-1">
        <InlineEdit
          value={defaultText(entity.description)}
          placeholder={t("descriptionPlaceholder")}
          onSave={onUpdateDescription}
          className="flex-1 text-foreground-muted"
        />
      </div>
      {isEdge && (
        <p className="mt-1 text-foreground-muted">
          {src} → {tgt}
          {edge?.cardinality && (
            <span className="ms-2">{t("cardinalityLabel", { value: edge.cardinality })}</span>
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
// LockedByOtherBanner — explicit visual + a11y signal that the
// inspected entity is held by someone else and any edit attempt
// will be ignored. The wrapper below also drops `pointer-events`
// on the body so click-driven mutations don't even reach the
// applyCommand calls.
// ---------------------------------------------------------------------------

function LockedByOtherBanner({
  projectId,
  heldBy,
}: {
  projectId: string;
  heldBy: string;
}) {
  const t = useTranslations("collaboration.lock");
  const presence = useCollabStore(selectPresence(projectId));
  const holderName =
    presence.find((p) => p.user_id === heldBy)?.user_name ?? heldBy;
  const color = colorFor(heldBy);
  return (
    <div
      role="status"
      className="flex items-center gap-2 border-b border-divider bg-surface-raised px-3 py-1.5 text-2xs text-foreground"
    >
      <span
        className="inline-block h-2 w-2 shrink-0 rounded-full"
        style={{ backgroundColor: color }}
        aria-hidden="true"
      />
      <span>{t("editingBy", { name: holderName })}</span>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Node detail (editable)
// ---------------------------------------------------------------------------

export function EntityDetail({
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
  const activeProject = useAppStore((s) => s.activeProject);
  const tab = useAppStore(
    (s) => (s.inspectorTabByKind.node_type ?? "definition") as InspectorFacetId,
  );
  const setTab = useCallback(
    (next: InspectorFacetId) =>
      useAppStore.getState().setInspectorTab("node_type", next),
    [],
  );
  const confirm = useConfirm();
  const t = useTranslations("inspector.tabs");
  const tEntity = useTranslations("inspector.entity");
  const tCommon = useTranslations("common");
  const ref = useEntityRef("node_type", node.id);
  const { inbound, outbound } = useEntityDependencies(ontology.id, ref);
  const lock = useEntityLock(activeProject?.id, node.id);
  const lockedByOther = lock.kind === "locked-by-other";

  const facetCtx: InspectorFacetContext = useMemo(
    () => ({
      ontology,
      kind: "node",
      entityRef: ref,
      entity: node,
      node,
      edge: null,
      gaps,
      inboundCount: inbound.length,
      outboundCount: outbound.length,
    }),
    [ontology, ref, node, gaps, inbound.length, outbound.length],
  );

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
      title: tEntity("node.deleteTitle"),
      description: tEntity("node.deleteDescription", { label: node.label }),
      confirmLabel: tCommon("delete"),
      variant: "danger",
    });
    if (!ok) return;
    applyCommand({ op: "delete_node", node_id: node.id });
    clearSelection();
    toast.success(tEntity("node.deletedToast", { label: node.label }));
  }, [applyCommand, confirm, node.id, node.label, clearSelection, tEntity, tCommon]);

  const visibleFacets = useMemo(
    () => visibleInspectorFacets(facetCtx),
    [facetCtx],
  );
  const tabs = useMemo(
    () =>
      visibleFacets.map((f) => ({
        id: f.id,
        label: t(f.labelKey),
        badge: f.badge?.(facetCtx),
      })),
    [visibleFacets, t, facetCtx],
  );
  // Fall back to the first visible facet when the persisted tab is
  // hidden for this entity (e.g. user was on `sample` for a lineage
  // node, then selected an edge — `sample` no longer applies).
  const activeFacet =
    inspectorFacetById(tab) && visibleFacets.some((f) => f.id === tab)
      ? inspectorFacetById(tab)!
      : visibleFacets[0];

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

      {lockedByOther && activeProject?.id && (
        <LockedByOtherBanner
          projectId={activeProject.id}
          heldBy={lock.heldBy}
        />
      )}

      <div className="border-b border-divider">
        <TabBar
          tabs={tabs}
          activeTab={activeFacet.id}
          onTabChange={(id) => setTab(id as InspectorFacetId)}
        />
      </div>

      <div
        className={cn(
          "min-h-0 flex-1 overflow-auto",
          lockedByOther && "pointer-events-none opacity-60",
        )}
        aria-disabled={lockedByOther || undefined}
      >
        {activeFacet.render(facetCtx)}
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
  const activeProject = useAppStore((s) => s.activeProject);
  const tab = useAppStore(
    (s) => (s.inspectorTabByKind.edge_type ?? "definition") as InspectorFacetId,
  );
  const setTab = useCallback(
    (next: InspectorFacetId) =>
      useAppStore.getState().setInspectorTab("edge_type", next),
    [],
  );
  const confirm = useConfirm();
  const t = useTranslations("inspector.tabs");
  const tEntity = useTranslations("inspector.entity");
  const tCommon = useTranslations("common");
  const ref = useEntityRef("edge_type", edge.id);
  const { inbound, outbound } = useEntityDependencies(ontology.id, ref);
  const lock = useEntityLock(activeProject?.id, edge.id);
  const lockedByOther = lock.kind === "locked-by-other";

  const facetCtx: InspectorFacetContext = useMemo(
    () => ({
      ontology,
      kind: "edge",
      entityRef: ref,
      entity: edge,
      node: null,
      edge,
      gaps,
      inboundCount: inbound.length,
      outboundCount: outbound.length,
    }),
    [ontology, ref, edge, gaps, inbound.length, outbound.length],
  );

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
      title: tEntity("edge.deleteTitle"),
      description: tEntity("edge.deleteDescription", { label: edge.label, source: src, target: tgt }),
      confirmLabel: tCommon("delete"),
      variant: "danger",
    });
    if (!ok) return;
    applyCommand({ op: "delete_edge", edge_id: edge.id });
    clearSelection();
    toast.success(tEntity("edge.deletedToast", { label: edge.label }));
  }, [applyCommand, confirm, edge.id, edge.label, clearSelection, src, tgt, tEntity, tCommon]);

  const visibleFacets = useMemo(
    () => visibleInspectorFacets(facetCtx),
    [facetCtx],
  );
  const tabs = useMemo(
    () =>
      visibleFacets.map((f) => ({
        id: f.id,
        label: t(f.labelKey),
        badge: f.badge?.(facetCtx),
      })),
    [visibleFacets, t, facetCtx],
  );
  const activeFacet =
    inspectorFacetById(tab) && visibleFacets.some((f) => f.id === tab)
      ? inspectorFacetById(tab)!
      : visibleFacets[0];

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

      {lockedByOther && activeProject?.id && (
        <LockedByOtherBanner
          projectId={activeProject.id}
          heldBy={lock.heldBy}
        />
      )}

      <div className="border-b border-divider">
        <TabBar
          tabs={tabs}
          activeTab={activeFacet.id}
          onTabChange={(id) => setTab(id as InspectorFacetId)}
        />
      </div>

      <div
        className={cn(
          "min-h-0 flex-1 overflow-auto",
          lockedByOther && "pointer-events-none opacity-60",
        )}
        aria-disabled={lockedByOther || undefined}
      >
        {activeFacet.render(facetCtx)}
      </div>
    </div>
  );
}
