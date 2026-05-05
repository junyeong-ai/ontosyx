"use client";

import { useCallback, useEffect, useState } from "react";
import { useTranslations } from "next-intl";
import { useAppStore, selectStateSelectedNodeId, selectStateSelectedEdgeId } from "@/lib/store";
import { applyOntologyCommands } from "@/lib/api";
import { isApiError } from "@/lib/api/client";
import { cn } from "@/lib/cn";
import { Redo2, Save, Undo2 } from "lucide-react";
import { Spinner } from "@/components/ui/spinner";
import { EmptyState } from "@/components/ui/empty-state";
import { toast } from "@/components/ui/toast";
import { Tooltip } from "@/components/ui/tooltip";
import { MergeBanner } from "@/components/collab/merge-banner";
import { CommandStackDiffDialog } from "@/components/collab/command-stack-diff-dialog";
import type { OntologyCommand, QualityGap } from "@/types/api";
import { EntityDetail, EdgeDetail } from "./entity-detail";
import { arr } from "@/lib/ir-collections";
import { gapTouchesEntity } from "@/lib/quality-utils";
import { useEntityLock } from "@/components/collab/use-entity-lock";
import { useEntityLockGuard } from "@/components/collab/use-entity-lock-guard";
import { selectStateLatestRemoteUpdate, selectStatePresence, useCollabStore } from "@/lib/collab";
import { useAuth } from "@/hooks/use-auth";

// ---------------------------------------------------------------------------
// Inspector — editable detail view for selected node or edge
// ---------------------------------------------------------------------------

export function InspectorPanel({ gaps }: { gaps: QualityGap[] }) {
  const t = useTranslations("inspector.toast");
  const tInspector = useTranslations("inspector");
  const ontology = useAppStore((s) => s.ontology);
  const applyOntologyDraftSnapshot = useAppStore((s) => s.applyOntologyDraftSnapshot);
  const selectedNodeId = useAppStore(selectStateSelectedNodeId);
  const selectedEdgeId = useAppStore(selectStateSelectedEdgeId);
  const commandStack = useAppStore((s) => s.commandStack);
  const redoStack = useAppStore((s) => s.redoStack);
  const undo = useAppStore((s) => s.undo);
  const redo = useAppStore((s) => s.redo);
  const activeOntologyDraft = useAppStore((s) => s.activeOntologyDraft);
  const [isSaving, setIsSaving] = useState(false);

  // Conflict surface — set when `applyOntologyCommands` returns 409
  // (revision moved on the server while the user was editing). The
  // banner stays mounted until the user resolves it via `Keep mine`
  // (rebase + retry) or `Take theirs` (drop local stack).
  //
  // BE contract for `remoteCommands`: the `EntityUpdated` WebSocket
  // event broadcast by the collaboration room when another client
  // commits ops carries `{ ontology_draft_id, base_revision, remote_revision,
  // commands }`. The room subscriber populates `remoteCommands` here
  // so the diff dialog can render the symmetric inventory. Until the
  // BE event lands, the field stays `undefined` and the dialog falls
  // back to its opaque "remote arrived" message — no FE work needed
  // when the BE ships.
  const [conflict, setConflict] = useState<{
    /** Display name of whoever shipped the remote update; falls back
     *  to "another user" when the WS room hasn't seen the actor's
     *  presence yet. */
    remoteAuthorName: string;
    /** Revision the local stack was authored against. */
    baseRevision: number;
    /** Revision the remote update brought in (best estimate — the
     *  server returns the new revision in the 409 body, but in
     *  practice we don't see it yet, so we surface `+1` as a
     *  placeholder). */
    remoteRevision: number;
    /** Remote commands between `baseRevision` → `remoteRevision`,
     *  oldest first. Populated when an `EntityUpdated` WS event has
     *  delivered the inventory; absent until BE ships the event. */
    remoteCommands?: readonly OntologyCommand[];
  } | null>(null);
  const [diffDialogOpen, setDiffDialogOpen] = useState(false);
  // Pull the current presence list to attribute the conflict —
  // the actor most likely is the only other active user in the
  // room. When the room has multiple collaborators the banner
  // shows "another user" as a generic fallback rather than guess.
  const presence = useCollabStore((s) =>
    activeOntologyDraft ? selectStatePresence(activeOntologyDraft.id)(s) : [],
  );
  const latestRemoteUpdate = useCollabStore((s) =>
    activeOntologyDraft ? selectStateLatestRemoteUpdate(activeOntologyDraft.id)(s) : undefined,
  );
  const ackRemoteUpdate = useCollabStore((s) => s.ackRemoteUpdate);
  // The current viewer's user id — distinct from `activeOntologyDraft.user_id`,
  // which is the project *owner*. A collaborator editing someone else's
  // project must filter their own presence row out using their *own* id,
  // not the project owner's, so the lone-remote heuristic and the
  // self-authored EntityUpdated guard both work in shared rooms.
  const auth = useAuth();
  const currentUserId = auth.user?.sub;

  // Verification state
  const verifications = useAppStore((s) => s.verifications);
  const loadVerifications = useAppStore((s) => s.loadVerifications);
  const verifyEl = useAppStore((s) => s.verifyElement);

  const ontologyId = ontology?.id ?? null;
  useEffect(() => {
    if (ontologyId) loadVerifications();
  }, [ontologyId, loadVerifications]);

  // Hold a collaboration lock on the currently inspected entity
  // for the lifetime of the panel mount — but only when no one
  // else already holds it; otherwise the inspector renders in
  // read-only mode and the guard stays inert so we don't fire
  // doomed `acquire_lock` frames every render.
  const lockedEntityId = selectedNodeId ?? selectedEdgeId ?? undefined;
  const liveLock = useEntityLock(activeOntologyDraft?.id, lockedEntityId);
  useEntityLockGuard(
    activeOntologyDraft?.id,
    lockedEntityId,
    liveLock.kind !== "locked-by-other",
  );

  const handleSave = useCallback(async () => {
    if (!activeOntologyDraft || commandStack.length === 0) return;
    setIsSaving(true);
    try {
      const commands = commandStack.map((e) => e.command);
      const resp = await applyOntologyCommands(activeOntologyDraft.id, {
        revision: activeOntologyDraft.revision,
        commands,
      });
      // Server canonical replaces local state + clears command stack
      // atomically — both halves can never drift.
      applyOntologyDraftSnapshot(resp.project);
      setConflict(null);
      toast.success(t("saved"));
    } catch (err) {
      // Revision conflict (409) — surface the merge banner instead
      // of a generic "save failed" toast. The user resolves via
      // Keep mine (rebase + retry) or Take theirs (discard local
      // edits + accept server state).
      if (isApiError(err) && err.kind() === "conflict") {
        // Prefer the realtime `EntityUpdated` snapshot when one
        // arrived — it carries the exact remote-ops inventory the
        // diff dialog needs for symmetric rendering, plus the
        // server-attributed author + true revision delta. The
        // self-authored guard skips frames echoing this client's
        // own previous save (the BE broadcasts to the room
        // including the author), which would otherwise mislabel the
        // remote actor as the local user. Falls back to presence-
        // based attribution when no usable WS frame has landed.
        const remote = latestRemoteUpdate;
        const remoteIsSelf =
          remote && currentUserId && remote.authorUserId === currentUserId;
        if (remote && !remoteIsSelf) {
          setConflict({
            remoteAuthorName: remote.authorUserName,
            baseRevision: remote.baseRevision,
            remoteRevision: remote.newRevision,
            remoteCommands: remote.commands,
          });
          return;
        }
        const others = currentUserId
          ? presence.filter((p) => p.user_id !== currentUserId)
          : presence;
        const remoteAuthorName =
          others.length === 1 ? others[0].user_name : t("conflictUnknownActor");
        setConflict({
          remoteAuthorName,
          baseRevision: activeOntologyDraft.revision,
          remoteRevision: activeOntologyDraft.revision + 1,
        });
        return;
      }
      toast.error(err instanceof Error ? err.message : t("saveFailed"));
    } finally {
      setIsSaving(false);
    }
  }, [
    activeOntologyDraft,
    commandStack,
    applyOntologyDraftSnapshot,
    latestRemoteUpdate,
    presence,
    currentUserId,
    t,
  ]);

  // Resolve the conflict by re-fetching the canonical project
  // and replaying the unsaved command stack on top. The server's
  // next save will use the fresher revision and either succeed or
  // surface a tighter conflict.
  const handleKeepLocal = useCallback(async () => {
    if (!activeOntologyDraft) return;
    try {
      const { getOntologyDraft } = await import("@/lib/api/ontology-drafts");
      const fresh = await getOntologyDraft(activeOntologyDraft.id);
      // `applyOntologyDraftSnapshot` replays the local commandStack atop
      // the new server snapshot — see ontology-slice for the
      // invariant.
      applyOntologyDraftSnapshot(fresh);
      setConflict(null);
      ackRemoteUpdate(activeOntologyDraft.id);
      void handleSave();
    } catch (err) {
      toast.error(err instanceof Error ? err.message : t("saveFailed"));
    }
  }, [activeOntologyDraft, applyOntologyDraftSnapshot, ackRemoteUpdate, handleSave, t]);

  // Drop the local stack and accept the server canonical verbatim.
  const handleAcceptRemote = useCallback(async () => {
    if (!activeOntologyDraft) return;
    try {
      const { getOntologyDraft } = await import("@/lib/api/ontology-drafts");
      const fresh = await getOntologyDraft(activeOntologyDraft.id);
      useAppStore.getState().clearCommandStack();
      applyOntologyDraftSnapshot(fresh);
      setConflict(null);
      ackRemoteUpdate(activeOntologyDraft.id);
    } catch (err) {
      toast.error(err instanceof Error ? err.message : t("saveFailed"));
    }
  }, [activeOntologyDraft, applyOntologyDraftSnapshot, ackRemoteUpdate, t]);


  // The body switches between "no ontology yet", "select something",
  // "selected entity not found in ontology", and the live detail view.
  // The outer `<aside id="inspector">` wraps every branch so the
  // skip-link target is always present in the DOM whenever the panel
  // is mounted — keeping `#inspector` resolvable even during transient
  // states that previously short-circuited the wrapper.
  const content = (() => {
    if (!ontology) {
      return <EmptyState variant="compact" title={tInspector("noOntology")} />;
    }
    if (selectedNodeId) {
      const node = arr(ontology.node_types).find((n) => n.id === selectedNodeId);
      if (!node) return <EmptyState variant="compact" title={tInspector("nodeNotFound")} />;
      const nodeGaps = gaps.filter((g) =>
        gapTouchesEntity(g, "node", selectedNodeId),
      );
      return (
        <EntityDetail
          node={node}
          ontology={ontology}
          gaps={nodeGaps}
          verifications={verifications}
          onVerify={() => ontologyId && verifyEl(node.id, "node")}
        />
      );
    }

    if (selectedEdgeId) {
      const edge = arr(ontology.edge_types).find((e) => e.id === selectedEdgeId);
      if (!edge) return <EmptyState variant="compact" title={tInspector("edgeNotFound")} />;
      const edgeGaps = gaps.filter((g) =>
        gapTouchesEntity(g, "edge", selectedEdgeId),
      );
      return (
        <EdgeDetail
          edge={edge}
          ontology={ontology}
          gaps={edgeGaps}
          verifications={verifications}
          onVerify={() => ontologyId && verifyEl(edge.id, "edge")}
        />
      );
    }

    return <EmptyState variant="compact" title={tInspector("selectPrompt")} />;
  })();

  return (
    <aside
      id="inspector"
      aria-label={tInspector("panelAria")}
      // `tabIndex={-1}` makes the skip-link target programmatically
      // focusable without adding the landmark itself to the tab cycle —
      // pressing Tab again lands on the first inspector control.
      tabIndex={-1}
      className="flex h-full flex-col outline-none focus-visible:ring-2 focus-visible:ring-brand-foreground/40 focus-visible:ring-inset"
    >
      {/* Undo/Redo toolbar — only visible when there's something actionable */}
      <div className={cn(
        "flex items-center gap-1 border-b border-divider px-2 py-1",
        commandStack.length === 0 && redoStack.length === 0 && "hidden",
      )}>
        <Tooltip content={tInspector("toolbar.undo")}>
          <button type="button"
            onClick={undo}
            disabled={commandStack.length === 0}
            aria-label={tInspector("toolbar.undo")}
            className="rounded p-1 text-foreground-muted hover:bg-surface-inset hover:text-foreground disabled:opacity-30"
          >
            <Undo2 className="h-3 w-3" />
          </button>
        </Tooltip>
        <Tooltip content={tInspector("toolbar.redo")}>
          <button type="button"
            onClick={redo}
            disabled={redoStack.length === 0}
            aria-label={tInspector("toolbar.redo")}
            className="rounded p-1 text-foreground-muted hover:bg-surface-inset hover:text-foreground disabled:opacity-30"
          >
            <Redo2 className="h-3 w-3" />
          </button>
        </Tooltip>
        {commandStack.length > 0 && (
          <>
            <span className="ms-auto text-2xs text-foreground-muted">
              {tInspector("toolbar.changes", { count: commandStack.length })}
              {!activeOntologyDraft && (
                <span className="ms-1 text-warning-foreground" title={tInspector("toolbar.unsaveableHint")}>
                  {tInspector("toolbar.unsaveable")}
                </span>
              )}
            </span>
            {activeOntologyDraft && (
              <Tooltip content={tInspector("toolbar.saveTooltip")}>
                <button type="button"
                  onClick={handleSave}
                  disabled={isSaving}
                  aria-label={tInspector("toolbar.save")}
                  className="rounded p-1 text-brand-foreground hover:bg-brand-surface hover:text-brand-foreground disabled:opacity-50"
                >
                  {isSaving ? (
                    <Spinner size="xs" />
                  ) : (
                    <Save className="h-3 w-3" />
                  )}
                </button>
              </Tooltip>
            )}
          </>
        )}
      </div>
      {conflict && (
        <div className="border-b border-divider p-2">
          <MergeBanner
            remoteAuthorName={conflict.remoteAuthorName}
            onKeepLocal={handleKeepLocal}
            onAcceptRemote={handleAcceptRemote}
            onCompare={() => setDiffDialogOpen(true)}
            busy={isSaving}
          />
        </div>
      )}
      <div className="flex-1 overflow-y-auto">{content}</div>
      {conflict && (
        <CommandStackDiffDialog
          open={diffDialogOpen}
          onOpenChange={setDiffDialogOpen}
          ontology={ontology}
          baseRevision={conflict.baseRevision}
          remoteRevision={conflict.remoteRevision}
          remoteAuthorName={conflict.remoteAuthorName}
          commandStack={commandStack}
          remoteCommands={conflict.remoteCommands}
          onKeepLocal={() => {
            setDiffDialogOpen(false);
            void handleKeepLocal();
          }}
          onAcceptRemote={() => {
            setDiffDialogOpen(false);
            void handleAcceptRemote();
          }}
          busy={isSaving}
        />
      )}
    </aside>
  );
}
