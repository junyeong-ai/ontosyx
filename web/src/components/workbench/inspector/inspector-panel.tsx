"use client";

import { useCallback, useEffect, useState } from "react";
import { useAppStore, selectSelectedNodeId, selectSelectedEdgeId } from "@/lib/store";
import { applyOntologyCommands } from "@/lib/api";
import { cn } from "@/lib/cn";
import { HugeiconsIcon } from "@hugeicons/react";
import { UndoIcon, RedoIcon, FloppyDiskIcon } from "@hugeicons/core-free-icons";
import { Spinner } from "@/components/ui/spinner";
import { toast } from "sonner";
import { Tooltip } from "@/components/ui/tooltip";
import type { OntologyIR, QualityGap } from "@/types/api";
import { NodeDetail, EdgeDetail } from "./entity-detail";

// ---------------------------------------------------------------------------
// Inspector — editable detail view for selected node or edge
// ---------------------------------------------------------------------------

export function InspectorPanel({ gaps }: { gaps: QualityGap[] }) {
  const ontology = useAppStore((s) => s.ontology);
  const setOntology = useAppStore((s) => s.setOntology);
  const selectedNodeId = useAppStore(selectSelectedNodeId);
  const selectedEdgeId = useAppStore(selectSelectedEdgeId);
  const commandStack = useAppStore((s) => s.commandStack);
  const redoStack = useAppStore((s) => s.redoStack);
  const undo = useAppStore((s) => s.undo);
  const redo = useAppStore((s) => s.redo);
  const activeProject = useAppStore((s) => s.activeProject);
  const setActiveProject = useAppStore((s) => s.setActiveProject);
  const [isSaving, setIsSaving] = useState(false);

  // Verification state
  const verifications = useAppStore((s) => s.verifications);
  const loadVerifications = useAppStore((s) => s.loadVerifications);
  const verifyEl = useAppStore((s) => s.verifyElement);

  const ontologyId = ontology?.id ?? null;
  useEffect(() => {
    if (ontologyId) loadVerifications(ontologyId);
  }, [ontologyId, loadVerifications]);

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
      setOntology(resp.project.ontology as OntologyIR);
      setActiveProject(resp.project);
      toast.success("Ontology saved");
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Failed to save");
    } finally {
      setIsSaving(false);
    }
  }, [activeProject, commandStack, setOntology, setActiveProject]);


  if (!ontology) return <Empty text="No ontology" />;

  const content = (() => {
    if (selectedNodeId) {
      const node = ontology.node_types.find((n) => n.id === selectedNodeId);
      if (!node) return <Empty text="Node not found" />;
      const nodeGaps = gaps.filter((g) => {
        const loc = g.location;
        return "node_id" in loc && loc.node_id === selectedNodeId && !("edge_id" in loc);
      });
      return (
        <NodeDetail
          node={node}
          ontology={ontology}
          gaps={nodeGaps}
          verifications={verifications}
          onVerify={() => ontologyId && verifyEl(ontologyId, node.id, "node")}
        />
      );
    }

    if (selectedEdgeId) {
      const edge = ontology.edge_types.find((e) => e.id === selectedEdgeId);
      if (!edge) return <Empty text="Edge not found" />;
      const edgeGaps = gaps.filter((g) => {
        const loc = g.location;
        return "edge_id" in loc && loc.edge_id === selectedEdgeId;
      });
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
        "flex items-center gap-1 border-b border-zinc-200 px-2 py-1 dark:border-zinc-800",
        commandStack.length === 0 && redoStack.length === 0 && "hidden",
      )}>
        <Tooltip content="Undo">
          <button
            onClick={undo}
            disabled={commandStack.length === 0}
            aria-label="Undo"
            className="rounded p-1 text-zinc-400 hover:bg-zinc-100 hover:text-zinc-600 disabled:opacity-30 dark:hover:bg-zinc-800"
          >
            <HugeiconsIcon icon={UndoIcon} className="h-3 w-3" size="100%" />
          </button>
        </Tooltip>
        <Tooltip content="Redo">
          <button
            onClick={redo}
            disabled={redoStack.length === 0}
            aria-label="Redo"
            className="rounded p-1 text-zinc-400 hover:bg-zinc-100 hover:text-zinc-600 disabled:opacity-30 dark:hover:bg-zinc-800"
          >
            <HugeiconsIcon icon={RedoIcon} className="h-3 w-3" size="100%" />
          </button>
        </Tooltip>
        {commandStack.length > 0 && (
          <>
            <span className="ml-auto text-[9px] text-zinc-400">
              {commandStack.length} change{commandStack.length !== 1 ? "s" : ""}
              {!activeProject && (
                <span className="ml-1 text-amber-500" title="No project — edits are local only. Open a project to enable saving.">
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
                  className="rounded p-1 text-emerald-500 hover:bg-emerald-50 hover:text-emerald-600 disabled:opacity-50 dark:hover:bg-emerald-950"
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
    <div className="flex h-full items-center justify-center text-xs text-zinc-400">
      {text}
    </div>
  );
}
