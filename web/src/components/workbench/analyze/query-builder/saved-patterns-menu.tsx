"use client";

import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";

import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { useConfirm } from "@/components/ui/confirm-dialog";
import { usePrompt } from "@/components/ui/prompt-dialog";
import {
  createSavedPattern,
  deleteSavedPattern,
  listSavedPatterns,
  updateSavedPattern,
  type SavedPattern,
} from "@/lib/api/queries";

// ---------------------------------------------------------------------------
// SavedPatternsMenu — save / load / rename / delete saved query patterns
// ---------------------------------------------------------------------------
//
// Lives next to the Clear / Run Query controls in the builder toolbar.
// Owns the transient "current pattern id" that lets the user update a
// loaded pattern in place; new patterns start with `null` and switch
// to the backend id after the first save.

export interface SavedPatternsMenuProps {
  /** Ontology the active pattern was authored against. Required — every
   *  saved pattern is tied to an ontology id so a reopen against the
   *  wrong schema is impossible. */
  ontologyId: string | null;
  /** `null` when the canvas hasn't been saved yet. After a successful
   *  save/load the builder threads back the backend id so "Save" can
   *  update in place instead of creating a duplicate. */
  currentId: string | null;
  /** Snapshot builder that returns the PatternIR JSON for the current
   *  canvas. Called only when the user clicks Save / Save As so the
   *  builder doesn't have to send a stale payload on every render. */
  getSnapshot: () => { pattern_ir: unknown; fallbackName?: string };
  /** Replace canvas state with a loaded pattern. */
  onLoad: (pattern: SavedPattern) => void;
  /** `null`s the current pattern id — triggered when the user deletes
   *  the currently-loaded pattern. */
  onCurrentIdCleared: () => void;
  /** Set after a successful Save or Save As. */
  onSaved: (pattern: SavedPattern) => void;
  /** Set when the user hits "New" so subsequent saves create fresh rows. */
  onNewPattern: () => void;
  /** Disabled state while the builder has nothing to save. */
  disabled?: boolean;
}

export function SavedPatternsMenu({
  ontologyId,
  currentId,
  getSnapshot,
  onLoad,
  onCurrentIdCleared,
  onSaved,
  onNewPattern,
  disabled,
}: SavedPatternsMenuProps) {
  const [isOpen, setIsOpen] = useState(false);
  const [items, setItems] = useState<SavedPattern[]>([]);
  const [loading, setLoading] = useState(false);
  const confirm = useConfirm();
  const prompt = usePrompt();

  // Refresh the list when the popover opens so the user sees recent
  // entries without a manual reload. Short-circuits when the current
  // ontology is unknown — we never list patterns across ontologies.
  useEffect(() => {
    if (!isOpen || !ontologyId) return;
    let cancelled = false;
    setLoading(true);
    listSavedPatterns(ontologyId, { limit: 50 })
      .then((page) => {
        if (!cancelled) setItems(page.items);
      })
      .catch((err) => {
        if (!cancelled) {
          toast.error("Failed to list saved patterns", {
            description: err instanceof Error ? err.message : String(err),
          });
        }
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [isOpen, ontologyId]);

  const refresh = useCallback(() => {
    if (!ontologyId) return;
    listSavedPatterns(ontologyId, { limit: 50 })
      .then((page) => setItems(page.items))
      .catch(() => {
        /* already surfaced via toast on initial load */
      });
  }, [ontologyId]);

  const handleSave = useCallback(async () => {
    if (!ontologyId) {
      toast.error("Load an ontology before saving a pattern");
      return;
    }
    const { pattern_ir, fallbackName } = getSnapshot();

    // Update-in-place when the canvas is tied to an existing pattern.
    if (currentId) {
      const existing = items.find((p) => p.id === currentId);
      try {
        await updateSavedPattern(currentId, {
          name: existing?.name ?? fallbackName ?? "Untitled pattern",
          description: existing?.description ?? undefined,
          pattern_ir,
        });
        toast.success(`Updated "${existing?.name ?? "pattern"}"`);
        refresh();
      } catch (err) {
        toast.error("Save failed", {
          description: err instanceof Error ? err.message : String(err),
        });
      }
      return;
    }

    // Otherwise prompt for a name and create a new row.
    const name = await prompt({
      title: "Save pattern",
      description: "Pick a name to find it later. Positions and layout are included.",
      defaultValue: fallbackName ?? "Untitled pattern",
      confirmLabel: "Save",
    });
    if (!name?.trim()) return;

    try {
      const saved = await createSavedPattern({
        name: name.trim(),
        ontology_id: ontologyId,
        pattern_ir,
      });
      toast.success(`Saved "${saved.name}"`);
      onSaved(saved);
      refresh();
    } catch (err) {
      toast.error("Save failed", {
        description: err instanceof Error ? err.message : String(err),
      });
    }
  }, [ontologyId, currentId, items, prompt, getSnapshot, onSaved, refresh]);

  const handleSaveAs = useCallback(async () => {
    if (!ontologyId) {
      toast.error("Load an ontology before saving a pattern");
      return;
    }
    const { pattern_ir, fallbackName } = getSnapshot();
    const name = await prompt({
      title: "Save pattern as…",
      description: "Creates a new saved pattern from the current canvas.",
      defaultValue: fallbackName ?? "Untitled pattern",
      confirmLabel: "Save",
    });
    if (!name?.trim()) return;
    try {
      const saved = await createSavedPattern({
        name: name.trim(),
        ontology_id: ontologyId,
        pattern_ir,
      });
      toast.success(`Saved "${saved.name}"`);
      onSaved(saved);
      refresh();
    } catch (err) {
      toast.error("Save failed", {
        description: err instanceof Error ? err.message : String(err),
      });
    }
  }, [ontologyId, prompt, getSnapshot, onSaved, refresh]);

  const handleDelete = useCallback(
    async (p: SavedPattern) => {
      const ok = await confirm({
        title: "Delete pattern",
        description: `Delete "${p.name}"? This cannot be undone.`,
        confirmLabel: "Delete",
        variant: "danger",
      });
      if (!ok) return;
      try {
        await deleteSavedPattern(p.id);
        toast.success(`Deleted "${p.name}"`);
        if (p.id === currentId) onCurrentIdCleared();
        refresh();
      } catch (err) {
        toast.error("Delete failed", {
          description: err instanceof Error ? err.message : String(err),
        });
      }
    },
    [confirm, currentId, onCurrentIdCleared, refresh],
  );

  const noOntology = !ontologyId;

  return (
    <>
      <button
        onClick={handleSave}
        disabled={disabled || noOntology}
        className="rounded px-2 py-0.5 text-[10px] font-medium text-muted-foreground transition-colors hover:bg-zinc-100 disabled:opacity-40 dark:hover:bg-zinc-800"
        title={currentId ? "Update the loaded pattern" : "Save current pattern"}
      >
        {currentId ? "Save" : "Save…"}
      </button>

      <Popover open={isOpen} onOpenChange={setIsOpen}>
        <PopoverTrigger className="cursor-pointer rounded px-2 py-0.5 text-[10px] font-medium text-muted-foreground transition-colors hover:bg-zinc-100 disabled:opacity-40 dark:hover:bg-zinc-800">
          Library
        </PopoverTrigger>
        <PopoverContent className="z-50 max-h-[70vh] w-72 overflow-auto rounded-lg border border-zinc-200 bg-white p-2 shadow-lg dark:border-zinc-700 dark:bg-zinc-900">
          <div className="flex items-center justify-between px-1 pb-2">
            <span className="text-[10px] font-semibold uppercase tracking-wider text-zinc-400">
              Saved patterns
            </span>
            <div className="flex items-center gap-1">
              <button
                onClick={() => {
                  onNewPattern();
                  setIsOpen(false);
                }}
                className="rounded px-1.5 py-0.5 text-[10px] text-zinc-500 hover:bg-zinc-100 dark:hover:bg-zinc-800"
                title="Start a new blank pattern"
              >
                New
              </button>
              <button
                onClick={() => {
                  setIsOpen(false);
                  void handleSaveAs();
                }}
                disabled={disabled || noOntology}
                className="rounded px-1.5 py-0.5 text-[10px] text-zinc-500 hover:bg-zinc-100 disabled:opacity-40 dark:hover:bg-zinc-800"
                title="Save the current canvas as a new pattern"
              >
                Save as…
              </button>
            </div>
          </div>

          {noOntology ? (
            <div className="p-2 text-xs text-muted-foreground">
              Load an ontology first.
            </div>
          ) : loading ? (
            <div className="p-2 text-xs text-muted-foreground">Loading…</div>
          ) : items.length === 0 ? (
            <div className="p-2 text-xs text-muted-foreground">
              No patterns yet. Use Save to capture your current canvas.
            </div>
          ) : (
            <ul className="flex flex-col gap-0.5">
              {items.map((p) => (
                <li
                  key={p.id}
                  className={`group flex items-center justify-between rounded px-2 py-1.5 text-xs ${
                    p.id === currentId
                      ? "bg-emerald-50 text-emerald-900 dark:bg-emerald-950/40 dark:text-emerald-200"
                      : "hover:bg-zinc-100 dark:hover:bg-zinc-800"
                  }`}
                >
                  <button
                    type="button"
                    onClick={() => {
                      onLoad(p);
                      setIsOpen(false);
                    }}
                    className="flex flex-1 flex-col text-left"
                    title={`Load "${p.name}"`}
                  >
                    <span className="truncate font-medium">{p.name}</span>
                    <span className="text-[10px] text-zinc-400">
                      {new Date(p.updated_at).toLocaleString()}
                    </span>
                  </button>
                  <button
                    type="button"
                    onClick={(e) => {
                      e.stopPropagation();
                      void handleDelete(p);
                    }}
                    className="ml-2 rounded px-1.5 py-0.5 text-[10px] text-zinc-400 opacity-0 transition-opacity hover:bg-red-100 hover:text-red-600 group-hover:opacity-100 dark:hover:bg-red-950/40 dark:hover:text-red-400"
                    title="Delete"
                  >
                    Delete
                  </button>
                </li>
              ))}
            </ul>
          )}
        </PopoverContent>
      </Popover>
    </>
  );
}
