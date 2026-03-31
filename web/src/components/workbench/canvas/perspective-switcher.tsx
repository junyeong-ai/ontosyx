"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { useReactFlow, type Node } from "@xyflow/react";
import { useAppStore } from "@/lib/store";
import {
  listPerspectives,
  savePerspective,
  deletePerspective,
} from "@/lib/api";
import type { WorkbenchPerspective } from "@/types/api";
import type { NodeGroup } from "@/lib/store/types";
import { cn } from "@/lib/cn";
import { useClickOutside } from "@/lib/use-click-outside";
import { toast } from "sonner";

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
  const ontology = useAppStore((s) => s.ontology);
  const activeProject = useAppStore((s) => s.activeProject);
  const restoreNodeGroups = useAppStore((s) => s.restoreNodeGroups);

  const { getViewport, setViewport } = useReactFlow();

  const [open, setOpen] = useState(false);
  const [perspectives, setPerspectives] = useState<WorkbenchPerspective[]>([]);
  const [activeName, setActiveName] = useState("Unsaved Layout");
  const [isSaving, setIsSaving] = useState(false);
  const [deleting, setDeleting] = useState<string | null>(null);
  const [showSaveAs, setShowSaveAs] = useState(false);
  const [newName, setNewName] = useState("");
  const dropdownRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  const lineageId = ontology?.id;

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
        project_id: activeProject?.id,
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
      toast.error("Failed to save perspective");
    } finally {
      setIsSaving(false);
    }
  }, [lineageId, newName, isSaving, nodes, getViewport, topologySignature, activeProject?.id, loadPerspectives]);

  const handleDelete = useCallback(
    async (perspective: WorkbenchPerspective) => {
      if (perspective.is_default) return;
      setDeleting(perspective.id);
      try {
        await deletePerspective(perspective.id);
        if (activeName === perspective.name) {
          setActiveName("Unsaved Layout");
        }
        await loadPerspectives();
      } catch {
        toast.error("Failed to delete perspective");
      } finally {
        setDeleting(null);
      }
    },
    [activeName, loadPerspectives],
  );

  if (!lineageId) return null;

  return (
    <div ref={dropdownRef} className="relative">
      {/* Trigger button */}
      <button
        onClick={() => {
          setOpen((v) => {
            if (!v) onOpen?.();
            return !v;
          });
        }}
        className={cn(
          "flex items-center gap-1 rounded-md border bg-white px-2 py-1 text-[10px] font-medium shadow-sm transition-colors",
          "border-zinc-200 text-zinc-600 hover:bg-zinc-50",
          "dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-400 dark:hover:bg-zinc-800",
        )}
      >
        <svg className="h-3 w-3" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5">
          <rect x="2" y="2" width="12" height="12" rx="2" />
          <path d="M2 6h12M6 6v8" />
        </svg>
        {activeName}
        <svg className={cn("h-2.5 w-2.5 transition-transform", open && "rotate-180")} viewBox="0 0 10 10" fill="none" stroke="currentColor" strokeWidth="1.5">
          <path d="M2 4l3 3 3-3" />
        </svg>
      </button>

      {/* Dropdown */}
      {open && (
        <div className={cn(
          "absolute left-0 top-full z-50 mt-1 w-48 rounded-lg border bg-white shadow-lg",
          "border-zinc-200 dark:border-zinc-700 dark:bg-zinc-900",
        )}>
          {/* Perspective list */}
          <div className="max-h-40 overflow-y-auto py-1">
            <div className="px-3 py-1 text-[9px] font-semibold uppercase tracking-wider text-zinc-400">
              Saved Views
            </div>
            {perspectives.length === 0 && (
              <div className="px-3 py-2 text-[10px] text-zinc-400">No saved views</div>
            )}
            {perspectives.map((p) => (
              <div
                key={p.id}
                className={cn(
                  "group flex items-center gap-1 px-3 py-1.5 text-[10px] hover:bg-zinc-50 dark:hover:bg-zinc-800",
                  activeName === p.name && "bg-zinc-50 dark:bg-zinc-800",
                )}
              >
                <button
                  onClick={() => handleSwitch(p)}
                  className="flex-1 truncate text-left text-zinc-700 dark:text-zinc-300"
                >
                  {p.name}
                  {p.is_default && (
                    <span className="ml-1 text-[8px] text-zinc-400">(default)</span>
                  )}
                </button>
                {!p.is_default && (
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      handleDelete(p);
                    }}
                    disabled={deleting === p.id}
                    className="hidden shrink-0 text-zinc-400 hover:text-red-500 group-hover:block disabled:opacity-50"
                  >
                    {deleting === p.id ? "..." : "\u00D7"}
                  </button>
                )}
              </div>
            ))}
          </div>

          {/* Save as */}
          <div className="border-t border-zinc-100 dark:border-zinc-800">
            {showSaveAs ? (
              <div className="flex items-center gap-1 px-2 py-1.5">
                <input
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
                  placeholder="Name..."
                  className="flex-1 rounded border border-zinc-200 bg-transparent px-1.5 py-0.5 text-[10px] text-zinc-700 outline-none placeholder:text-zinc-500 dark:border-zinc-700 dark:text-zinc-300"
                />
                <button
                  onClick={handleSaveAs}
                  disabled={!newName.trim() || isSaving}
                  className="rounded bg-emerald-600 px-2 py-0.5 text-[10px] text-white hover:bg-emerald-700 disabled:opacity-50"
                >
                  {isSaving ? "..." : "Save"}
                </button>
              </div>
            ) : (
              <button
                onClick={() => setShowSaveAs(true)}
                className="w-full px-3 py-1.5 text-left text-[10px] text-zinc-500 hover:bg-zinc-50 hover:text-zinc-700 dark:hover:bg-zinc-800 dark:hover:text-zinc-300"
              >
                Save as...
              </button>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
