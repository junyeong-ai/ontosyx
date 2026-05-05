"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslations } from "next-intl";
import { useReactFlow, type Node } from "@xyflow/react";
import { ArrowDown } from "lucide-react";
import { KanbanSquare } from "lucide-react";
import { useAppStore } from "@/lib/store";
import {
  listPerspectives,
  savePerspective,
  deletePerspective,
} from "@/lib/api";
import type { WorkbenchPerspective } from "@/types/api";
import type { NodeGroup } from "@/lib/store/types";
import { cn } from "@/lib/cn";
import { FormInput } from "@/components/ui/form-input";
import { useClickOutside } from "@/hooks/use-click-outside";
import { toast } from "@/components/ui/toast";

// ---------------------------------------------------------------------------
// Perspective Switcher — small dropdown on the canvas
// ---------------------------------------------------------------------------

export function PerspectiveSwitcher({
  nodes,
  topologySignature,
  onApplyPositions,
  onOpen,
}: {
  nodes: Node[];
  topologySignature: string;
  onApplyPositions: (positions: Record<string, { x: number; y: number }>) => void;
  onOpen?: () => void;
}) {
  const t = useTranslations("workbench.canvas.perspective");
  const tCommon = useTranslations("common");
  const ontology = useAppStore((s) => s.ontology);
  const activeProject = useAppStore((s) => s.activeProject);
  const restoreNodeGroups = useAppStore((s) => s.restoreNodeGroups);

  const { getViewport, setViewport } = useReactFlow();

  const [open, setOpen] = useState(false);
  const [perspectives, setPerspectives] = useState<WorkbenchPerspective[]>([]);
  const [activeName, setActiveName] = useState(t("unsaved"));
  const [isSaving, setIsSaving] = useState(false);
  const [deleting, setDeleting] = useState<string | null>(null);
  const [showSaveAs, setShowSaveAs] = useState(false);
  const [newName, setNewName] = useState("");
  const dropdownRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  const lineageId = ontology?.id;
  const activeProjectId = activeProject?.id;

  // Load perspectives when dropdown opens
  const loadPerspectives = useCallback(async () => {
    if (!lineageId) return;
    try {
      const list = await listPerspectives(lineageId);
      setPerspectives(list);
    } catch {
      // silently fail
    }
  }, [lineageId]);

  useEffect(() => {
    if (open) {
      loadPerspectives();
    }
  }, [open, loadPerspectives]);

  // Close on click outside
  const handleClickOutside = useCallback(() => {
    setOpen(false);
    setShowSaveAs(false);
    setNewName("");
  }, []);
  useClickOutside(dropdownRef, handleClickOutside, open);

  // Focus input when save-as opens
  useEffect(() => {
    if (showSaveAs) {
      setTimeout(() => inputRef.current?.focus(), 50);
    }
  }, [showSaveAs]);

  const handleSwitch = useCallback(
    (perspective: WorkbenchPerspective) => {
      // Apply saved node positions
      const positions = perspective.positions as Record<string, { x: number; y: number }>;
      if (positions && Object.keys(positions).length > 0) {
        onApplyPositions(positions);
      }

      // Apply saved viewport (don't fitView — use exact saved viewport)
      if (perspective.viewport) {
        setViewport(
          { x: perspective.viewport.x, y: perspective.viewport.y, zoom: perspective.viewport.zoom },
          { duration: 300 },
        );
      }

      // Restore node groups from saved filters
      const groups = (perspective.filters as { groups?: Record<string, NodeGroup> })?.groups;
      if (groups && Object.keys(groups).length > 0) {
        restoreNodeGroups(groups);
      }

      setActiveName(perspective.name);
      setOpen(false);
    },
    [setViewport, onApplyPositions, restoreNodeGroups],
  );

  const handleSaveAs = useCallback(async () => {
    if (!lineageId || !newName.trim() || isSaving) return;
    setIsSaving(true);
    try {
      const positions: Record<string, { x: number; y: number }> = {};
      for (const n of nodes) {
        positions[n.id] = { x: n.position.x, y: n.position.y };
      }
      const vp = getViewport();
      const groups = useAppStore.getState().nodeGroups;
      const collapsedGroupIds = Object.entries(groups)
        .filter(([, g]) => g.collapsed)
        .map(([id]) => id);
      await savePerspective({
        lineage_id: lineageId,
        topology_signature: topologySignature,
        ontology_draft_id: activeProjectId,
        name: newName.trim(),
        positions,
        viewport: { x: vp.x, y: vp.y, zoom: vp.zoom },
        filters: { groups },
        collapsed_groups: collapsedGroupIds,
        is_default: false,
      });
      setActiveName(newName.trim());
      setNewName("");
      setShowSaveAs(false);
      await loadPerspectives();
    } catch {
      toast.error(t("saveFailed"));
    } finally {
      setIsSaving(false);
    }
  }, [lineageId, newName, isSaving, nodes, getViewport, topologySignature, activeProjectId, loadPerspectives, t]);

  const handleDelete = useCallback(
    async (perspective: WorkbenchPerspective) => {
      if (perspective.is_default) return;
      setDeleting(perspective.id);
      try {
        await deletePerspective(perspective.id);
        if (activeName === perspective.name) {
          setActiveName(t("unsaved"));
        }
        await loadPerspectives();
      } catch {
        toast.error(t("deleteFailed"));
      } finally {
        setDeleting(null);
      }
    },
    [activeName, loadPerspectives, t],
  );

  if (!lineageId) return null;

  return (
    <div ref={dropdownRef} className="relative">
      {/* Trigger button — label is capped at 9rem so a long
          perspective name (or the "unsaved layout" sentinel)
          can't push downstream toolbar siblings off the canvas. */}
      <button type="button"
        onClick={() => {
          setOpen((v) => {
            if (!v) onOpen?.();
            return !v;
          });
        }}
        title={activeName}
        className={cn(
          "flex max-w-[12rem] items-center gap-1 rounded-md border bg-surface-base px-2 py-1 text-2xs font-medium shadow-1 transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)]",
          "border-divider text-foreground hover:bg-surface-raised",
        )}
      >
        <KanbanSquare className="h-3 w-3 shrink-0" />
        <span className="min-w-0 truncate">{activeName}</span>
        <ArrowDown className={cn(
 "h-2.5 w-2.5 shrink-0 transition-transform duration-[var(--duration-quick)] ease-[var(--ease-out)]",
 open && "rotate-180",
 )} />
      </button>

      {/* Dropdown */}
      {open && (
        <div className={cn(
          "absolute start-0 top-full z-popover mt-1 w-48 rounded-lg border bg-surface-base shadow-3",
          "border-divider",
        )}>
          {/* Perspective list */}
          <div className="max-h-40 overflow-y-auto py-1">
            <div className="px-3 py-1 text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
              {t("savedViews")}
            </div>
            {perspectives.length === 0 && (
              <div className="px-3 py-2 text-2xs text-foreground-muted">{t("noSavedViews")}</div>
            )}
            {perspectives.map((p) => (
              <div
                key={p.id}
                className={cn(
                  "group flex items-center gap-1 px-3 py-1.5 text-2xs hover:bg-surface-raised",
                  activeName === p.name && "bg-surface-raised",
                )}
              >
                <button type="button"
                  onClick={() => handleSwitch(p)}
                  className="flex-1 truncate text-start text-foreground"
                >
                  {p.name}
                  {p.is_default && (
                    <span className="ms-1 text-2xs text-foreground-muted">{t("defaultTag")}</span>
                  )}
                </button>
                {!p.is_default && (
                  <button type="button"
                    onClick={(e) => {
                      e.stopPropagation();
                      handleDelete(p);
                    }}
                    disabled={deleting === p.id}
                    className="hidden shrink-0 text-foreground-muted hover:text-danger-foreground group-hover:block disabled:opacity-50"
                  >
                    {deleting === p.id ? tCommon("deleting") : "\u00D7"}
                  </button>
                )}
              </div>
            ))}
          </div>

          {/* Save as */}
          <div className="border-t border-divider-soft">
            {showSaveAs ? (
              <div className="flex items-center gap-1 px-2 py-1.5">
                <FormInput
                  ref={inputRef}
                  type="text"
                  value={newName}
                  onChange={(e) => setNewName(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") handleSaveAs();
                    if (e.key === "Escape") {
                      setShowSaveAs(false);
                      setNewName("");
                    }
                  }}
                  placeholder={t("namePlaceholder")}
                  density="compact"
                  className="flex-1"
                />
                <button type="button"
                  onClick={handleSaveAs}
                  disabled={!newName.trim() || isSaving}
                  className="rounded bg-brand-solid px-2 py-0.5 text-2xs text-foreground-onbrand hover:bg-brand-solid disabled:opacity-50"
                >
                  {isSaving ? tCommon("saving") : tCommon("save")}
                </button>
              </div>
            ) : (
              <button type="button"
                onClick={() => setShowSaveAs(true)}
                className="w-full px-3 py-1.5 text-start text-2xs text-foreground-muted hover:bg-surface-raised hover:text-foreground-muted"
              >
                {t("saveAs")}
              </button>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
