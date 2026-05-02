"use client";

import { useCallback, useEffect, useState } from "react";
import { useTranslations } from "next-intl";
import { useAppStore, selectStateSelectedNodeId, selectStateSelectedEdgeId } from "@/lib/store";
import { applyOntologyCommands } from "@/lib/api";
import { cn } from "@/lib/cn";
import { HugeiconsIcon } from "@hugeicons/react";
import { UndoIcon, RedoIcon, FloppyDiskIcon } from "@hugeicons/core-free-icons";
import { Spinner } from "@/components/ui/spinner";
import { toast } from "sonner";
import { Tooltip } from "@/components/ui/tooltip";
import type { QualityGap } from "@/types/api";
import { EntityDetail, EdgeDetail } from "./entity-detail";
import { arr } from "@/lib/ir-collections";
import { gapTouchesEntity } from "@/lib/quality-utils";
import { useEntityLock } from "@/components/collab/use-entity-lock";
import { useEntityLockGuard } from "@/components/collab/use-entity-lock-guard";

// ---------------------------------------------------------------------------
// Inspector — editable detail view for selected node or edge
// ---------------------------------------------------------------------------

export function InspectorPanel({ gaps }: { gaps: QualityGap[] }) {
  const t = useTranslations("inspector.toast");
  const ontology = useAppStore((s) => s.ontology);
  const applyProjectSnapshot = useAppStore((s) => s.applyProjectSnapshot);
  const selectedNodeId = useAppStore(selectStateSelectedNodeId);
  const selectedEdgeId = useAppStore(selectStateSelectedEdgeId);
  const commandStack = useAppStore((s) => s.commandStack);
  const redoStack = useAppStore((s) => s.redoStack);
  const undo = useAppStore((s) => s.undo);
  const redo = useAppStore((s) => s.redo);
  const activeProject = useAppStore((s) => s.activeProject);
  const [isSaving, setIsSaving] = useState(false);

  // Verification state
  const verifications = useAppStore((s) => s.verifications);
  const loadVerifications = useAppStore((s) => s.loadVerifications);
  const verifyEl = useAppStore((s) => s.verifyElement);

  const ontologyId = ontology?.id ?? null;
  useEffect(() => {
    if (ontologyId) loadVerifications(ontologyId);
  }, [ontologyId, loadVerifications]);

  // Hold a collaboration lock on the currently inspected entity
  // for the lifetime of the panel mount — but only when no one
  // else already holds it; otherwise the inspector renders in
  // read-only mode and the guard stays inert so we don't fire
  // doomed `acquire_lock` frames every render.
  const lockedEntityId = selectedNodeId ?? selectedEdgeId ?? undefined;
  const liveLock = useEntityLock(activeProject?.id, lockedEntityId);
  useEntityLockGuard(
    activeProject?.id,
    lockedEntityId,
    liveLock.kind !== "locked-by-other",
  );

  const handleSave = useCallback(async () => {
    if (!activeProject || commandStack.length === 0) return;
    setIsSaving(true);
    try {
      const commands = commandStack.map((e) => e.command);
      const resp = await applyOntologyCommands(activeProject.id, {
        revision: activeProject.revision,
        commands,
      });
      // Server canonical replaces local state + clears command stack
      // atomically — both halves can never drift.
      applyProjectSnapshot(resp.project);
      toast.success(t("saved"));
    } catch (err) {
      toast.error(err instanceof Error ? err.message : t("saveFailed"));
    } finally {
      setIsSaving(false);
    }
  }, [activeProject, commandStack, applyProjectSnapshot]);


  if (!ontology) return <Empty text="No ontology" />;

  const content = (() => {
    if (selectedNodeId) {
      const node = arr(ontology.node_types).find((n) => n.id === selectedNodeId);
      if (!node) return <Empty text="Node not found" />;
      const nodeGaps = gaps.filter((g) =>
        gapTouchesEntity(g, "node", selectedNodeId),
      );
      return (
        <EntityDetail
          node={node}
          ontology={ontology}
          gaps={nodeGaps}
          verifications={verifications}
          onVerify={() => ontologyId && verifyEl(ontologyId, node.id, "node")}
        />
      );
    }

    if (selectedEdgeId) {
      const edge = arr(ontology.edge_types).find((e) => e.id === selectedEdgeId);
      if (!edge) return <Empty text="Edge not found" />;
      const edgeGaps = gaps.filter((g) =>
        gapTouchesEntity(g, "edge", selectedEdgeId),
      );
      return (
        <EdgeDetail
          edge={edge}
          ontology={ontology}
          gaps={edgeGaps}
          verifications={verifications}
          onVerify={() => ontologyId && verifyEl(ontologyId, edge.id, "edge")}
        />
      );
    }

    return <Empty text="Select a node or edge" />;
  })();

  return (
    <div className="flex h-full flex-col">
      {/* Undo/Redo toolbar — only visible when there's something actionable */}
      <div className={cn(
        "flex items-center gap-1 border-b border-divider px-2 py-1",
        commandStack.length === 0 && redoStack.length === 0 && "hidden",
      )}>
        <Tooltip content="Undo">
          <button
            onClick={undo}
            disabled={commandStack.length === 0}
            aria-label="Undo"
            className="rounded p-1 text-muted-foreground hover:bg-surface-inset hover:text-foreground disabled:opacity-30"
          >
            <HugeiconsIcon icon={UndoIcon} className="h-3 w-3" size="100%" />
          </button>
        </Tooltip>
        <Tooltip content="Redo">
          <button
            onClick={redo}
            disabled={redoStack.length === 0}
            aria-label="Redo"
            className="rounded p-1 text-muted-foreground hover:bg-surface-inset hover:text-foreground disabled:opacity-30"
          >
            <HugeiconsIcon icon={RedoIcon} className="h-3 w-3" size="100%" />
          </button>
        </Tooltip>
        {commandStack.length > 0 && (
          <>
            <span className="ml-auto text-2xs text-muted-foreground">
              {commandStack.length} change{commandStack.length !== 1 ? "s" : ""}
              {!activeProject && (
                <span className="ml-1 text-warning-foreground" title="No project — edits are local only. Open a project to enable saving.">
                  (unsaveable)
                </span>
              )}
            </span>
            {activeProject && (
              <Tooltip content="Save to server (⌘S)">
                <button
                  onClick={handleSave}
                  disabled={isSaving}
                  aria-label="Save to server"
                  className="rounded p-1 text-brand-foreground hover:bg-brand-surface hover:text-brand-foreground disabled:opacity-50 dark:hover:bg-brand-surface"
                >
                  {isSaving ? (
                    <Spinner size="xs" />
                  ) : (
                    <HugeiconsIcon icon={FloppyDiskIcon} className="h-3 w-3" size="100%" />
                  )}
                </button>
              </Tooltip>
            )}
          </>
        )}
      </div>
      <div className="flex-1 overflow-y-auto">{content}</div>
    </div>
  );
}

function Empty({ text }: { text: string }) {
  return (
    <div className="flex h-full items-center justify-center text-xs text-muted-foreground">
      {text}
    </div>
  );
}
