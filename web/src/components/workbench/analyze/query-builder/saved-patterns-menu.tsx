"use client";

import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslations } from "next-intl";
import { toast } from "sonner";
import { useQueryClient } from "@tanstack/react-query";

import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { useConfirm } from "@/components/providers/confirm-provider";
import { usePrompt } from "@/components/providers/prompt-provider";
import {
  createSavedPattern,
  deleteSavedPattern,
  updateSavedPattern,
  type SavedPattern,
} from "@/lib/api/queries";
import {
  savedPatternsKeys,
  useSavedPatterns,
} from "@/hooks/api/use-saved-patterns";

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
  /** `true` when the canvas has unsaved edits relative to the loaded
   *  pattern (or any content for a brand-new canvas). Renders a
   *  discreet dot next to the Save button. */
  isDirty?: boolean;
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
  isDirty,
}: SavedPatternsMenuProps) {
  const t = useTranslations("workbench.queryBuilder.savedPatterns");
  const tCommon = useTranslations("common");
  const [isOpen, setIsOpen] = useState(false);
  const confirm = useConfirm();
  const prompt = usePrompt();
  const qc = useQueryClient();

  // Fetch only while the popover is open — `enabled` gates the query so
  // a closed menu doesn't burn bandwidth. Tanstack Query retains the
  // result for its default staleTime, so a quick close/reopen reuses
  // the cached list instead of re-fetching.
  const { data, isFetching, isError, error } = useSavedPatterns(
    ontologyId,
    { limit: 50 },
    { enabled: isOpen && !!ontologyId },
  );
  // Stable reference across renders — otherwise the `?? []` fallback
  // allocates a fresh empty array each render, invalidating every
  // `useCallback` downstream that depends on `items`.
  const items = useMemo<SavedPattern[]>(() => data?.items ?? [], [data]);
  const loading = isFetching;

  useEffect(() => {
    if (isError) {
      toast.error(t("listFailed"), {
        description: error instanceof Error ? error.message : String(error),
      });
    }
  }, [isError, error, t]);

  const refresh = useCallback(() => {
    if (!ontologyId) return;
    qc.invalidateQueries({
      queryKey: savedPatternsKeys.list(ontologyId, { limit: 50 }),
    });
  }, [qc, ontologyId]);

  const handleSave = useCallback(async () => {
    if (!ontologyId) {
      toast.error(t("loadOntologyFirst"));
      return;
    }
    const { pattern_ir, fallbackName } = getSnapshot();

    // Update-in-place when the canvas is tied to an existing pattern.
    if (currentId) {
      const existing = items.find((p) => p.id === currentId);
      try {
        await updateSavedPattern(currentId, {
          name: existing?.name ?? fallbackName ?? t("untitledPattern"),
          description: existing?.description ?? undefined,
          pattern_ir,
        });
        toast.success(
          t("updateSuccess", {
            name: existing?.name ?? t("updatePattern"),
          }),
        );
        refresh();
      } catch (err) {
        toast.error(t("saveFailed"), {
          description: err instanceof Error ? err.message : String(err),
        });
      }
      return;
    }

    // Otherwise prompt for a name and create a new row.
    const name = await prompt({
      title: t("savePromptTitle"),
      description: t("savePromptDescription"),
      defaultValue: fallbackName ?? t("untitledPattern"),
      confirmLabel: t("savePromptConfirm"),
    });
    if (!name?.trim()) return;

    try {
      const saved = await createSavedPattern({
        name: name.trim(),
        ontology_lineage_id: ontologyId,
        pattern_ir,
      });
      toast.success(t("saveSuccess", { name: saved.name }));
      onSaved(saved);
      refresh();
    } catch (err) {
      toast.error(t("saveFailed"), {
        description: err instanceof Error ? err.message : String(err),
      });
    }
  }, [ontologyId, currentId, items, prompt, getSnapshot, onSaved, refresh, t]);

  const handleSaveAs = useCallback(async () => {
    if (!ontologyId) {
      toast.error(t("loadOntologyFirst"));
      return;
    }
    const { pattern_ir, fallbackName } = getSnapshot();
    const name = await prompt({
      title: t("saveAsPromptTitle"),
      description: t("saveAsPromptDescription"),
      defaultValue: fallbackName ?? t("untitledPattern"),
      confirmLabel: t("savePromptConfirm"),
    });
    if (!name?.trim()) return;
    try {
      const saved = await createSavedPattern({
        name: name.trim(),
        ontology_lineage_id: ontologyId,
        pattern_ir,
      });
      toast.success(t("saveSuccess", { name: saved.name }));
      onSaved(saved);
      refresh();
    } catch (err) {
      toast.error(t("saveFailed"), {
        description: err instanceof Error ? err.message : String(err),
      });
    }
  }, [ontologyId, prompt, getSnapshot, onSaved, refresh, t]);

  const handleDelete = useCallback(
    async (p: SavedPattern) => {
      const ok = await confirm({
        title: t("deletePromptTitle"),
        description: t("deletePromptDescription", { name: p.name }),
        confirmLabel: t("deletePromptConfirm"),
        variant: "danger",
      });
      if (!ok) return;
      try {
        await deleteSavedPattern(p.id);
        toast.success(t("deleteSuccess", { name: p.name }));
        if (p.id === currentId) onCurrentIdCleared();
        refresh();
      } catch (err) {
        toast.error(t("deleteFailed"), {
          description: err instanceof Error ? err.message : String(err),
        });
      }
    },
    [confirm, currentId, onCurrentIdCleared, refresh, t],
  );

  const noOntology = !ontologyId;

  return (
    <>
      <button
        onClick={handleSave}
        disabled={disabled || noOntology}
        className="flex items-center gap-1 rounded px-2 py-0.5 text-2xs font-medium text-muted-foreground transition-colors hover:bg-surface-inset disabled:opacity-40"
        title={
          currentId
            ? isDirty
              ? t("updateDirtyTitle")
              : t("updateTitle")
            : t("saveTitle")
        }
      >
        <span>{currentId ? tCommon("save") : t("saveWithEllipsis")}</span>
        {isDirty && (
          <span
            aria-label={t("unsavedChanges")}
            className="h-1.5 w-1.5 rounded-full bg-warning-foreground"
          />
        )}
      </button>
      <button
        onClick={handleSaveAs}
        disabled={disabled || noOntology}
        className="rounded px-2 py-0.5 text-2xs font-medium text-muted-foreground transition-colors hover:bg-surface-inset disabled:opacity-40"
        title={t("saveAsTitle")}
      >
        {t("saveAs")}
      </button>

      <Popover open={isOpen} onOpenChange={setIsOpen}>
        <PopoverTrigger className="cursor-pointer rounded px-2 py-0.5 text-2xs font-medium text-muted-foreground transition-colors hover:bg-surface-inset disabled:opacity-40">
          {t("library")}
        </PopoverTrigger>
        <PopoverContent className="z-50 max-h-[70vh] w-72 overflow-auto rounded-lg border border-divider bg-surface-base p-2 shadow-lg">
          <div className="flex items-center justify-between px-1 pb-2">
            <span className="text-2xs font-semibold uppercase tracking-wider text-muted-foreground">
              {t("listTitle")}
            </span>
            <div className="flex items-center gap-1">
              <button
                onClick={() => {
                  onNewPattern();
                  setIsOpen(false);
                }}
                className="rounded px-1.5 py-0.5 text-2xs text-foreground-muted hover:bg-surface-inset"
                title={t("newPatternTitle")}
              >
                {t("newPattern")}
              </button>
              <button
                onClick={() => {
                  setIsOpen(false);
                  void handleSaveAs();
                }}
                disabled={disabled || noOntology}
                className="rounded px-1.5 py-0.5 text-2xs text-foreground-muted hover:bg-surface-inset disabled:opacity-40"
                title={t("saveAsLibraryTitle")}
              >
                {t("saveAs")}
              </button>
            </div>
          </div>

          {noOntology ? (
            <div className="p-2 text-xs text-muted-foreground">
              {t("loadOntologyHint")}
            </div>
          ) : loading ? (
            <div className="p-2 text-xs text-muted-foreground">{tCommon("loading")}</div>
          ) : items.length === 0 ? (
            <div className="p-2 text-xs text-muted-foreground">
              {t("empty")}
            </div>
          ) : (
            <ul className="flex flex-col gap-0.5">
              {items.map((p) => (
                <li
                  key={p.id}
                  className={`group flex items-center justify-between rounded px-2 py-1.5 text-xs ${
                    p.id === currentId
                      ? "bg-brand-surface text-brand-foreground-strong-strong"
                      : "hover:bg-surface-inset"
                  }`}
                >
                  <button
                    type="button"
                    onClick={() => {
                      onLoad(p);
                      setIsOpen(false);
                    }}
                    className="flex flex-1 flex-col text-left"
                    title={t("loadTitle", { name: p.name })}
                  >
                    <span className="truncate font-medium">{p.name}</span>
                    <span className="text-2xs text-muted-foreground">
                      {new Date(p.updated_at).toLocaleString()}
                    </span>
                  </button>
                  <button
                    type="button"
                    onClick={(e) => {
                      e.stopPropagation();
                      void handleDelete(p);
                    }}
                    className="ml-2 rounded px-1.5 py-0.5 text-2xs text-muted-foreground opacity-0 transition-opacity hover:bg-danger-surface hover:text-danger-foreground group-hover:opacity-100 dark:hover:bg-danger-surface dark:hover:text-danger-foreground"
                    title={t("deleteTitle")}
                  >
                    {t("deleteTitle")}
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
