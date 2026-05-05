"use client";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslations } from "next-intl";
import { useAppStore } from "@/lib/store";
import {
  ApiError,
  isPendingReconcile,
  refineOntologyDraft,
  editOntologyDraft,
} from "@/lib/api";
import { Check, Repeat, Wand2, X } from "lucide-react";
import { Pencil } from "lucide-react";
import { Spinner } from "@/components/ui/spinner";
import { KeyboardShortcut } from "@/components/ui/keyboard-shortcut";
import { CommandPreview } from "./command-preview";
import { cn } from "@/lib/cn";
import { toast } from "@/components/ui/toast";
import { TOAST_WARNING } from "@/lib/toast/durations";
import { useConfirm } from "@/components/providers/confirm-provider";
import type { OntologyCommand } from "@/types/api";

// Re-export extracted components for backward compatibility
export { DiffOverlayBar } from "./diff-overlay-bar";
export { VersionDiffBar } from "./version-diff-bar";

// ---------------------------------------------------------------------------
// Loading hints — rotate tips while LLM processes
// ---------------------------------------------------------------------------

function LoadingHint({ baseMessage }: { baseMessage: string }) {
  const t = useTranslations("workbench.canvas.commandBar");
  // `t.raw` returns the underlying JSON value — an array of tips — without
  // formatting. Memoised here to keep a stable reference across renders;
  // otherwise the rotating `setInterval` would trip up on identity changes.
  const tips = useMemo(() => t.raw("loadingTips") as string[], [t]);
  const [tipIndex, setTipIndex] = useState(0);
  const [showTip, setShowTip] = useState(false);

  useEffect(() => {
    // Show first tip after 2 seconds
    const showTimer = setTimeout(() => setShowTip(true), 2000);
    // Rotate tips every 3 seconds
    const rotateTimer = setInterval(() => {
      setTipIndex((prev) => (prev + 1) % tips.length);
    }, 3000);
    return () => {
      clearTimeout(showTimer);
      clearInterval(rotateTimer);
    };
  }, [tips.length]);

  return (
    <div className="flex items-center gap-1.5">
      <span className="text-2xs text-foreground-muted">{baseMessage}</span>
      {showTip && (
        <span className="text-2xs text-foreground-muted transition-opacity duration-[var(--duration-slow)] ease-[var(--ease-out)]">
          · {tips[tipIndex]}
        </span>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Command bar mode
// ---------------------------------------------------------------------------

type CommandMode = "edit" | "refine";

// ---------------------------------------------------------------------------
// State machine for the command bar phases
// ---------------------------------------------------------------------------

type Phase =
  | { type: "input" }
  | { type: "loading"; message: string }
  | {
      type: "preview";
      commands: OntologyCommand[];
      explanation: string;
    };

// ---------------------------------------------------------------------------
// "Ask Ontosyx" — floating LLM command bar on the canvas (Edit + Refine modes)
// ---------------------------------------------------------------------------

export function CommandBar() {
  const t = useTranslations("workbench.canvas.commandBar");
  const tCommon = useTranslations("common");
  const activeProject = useAppStore((s) => s.activeProject);
  const applyProjectSnapshot = useAppStore((s) => s.applyProjectSnapshot);
  const setLastReconcileReport = useAppStore((s) => s.setLastReconcileReport);
  const applyCommand = useAppStore((s) => s.applyCommand);
  const commandStack = useAppStore((s) => s.commandStack);
  const ontology = useAppStore((s) => s.ontology);

  const confirmDialog = useConfirm();
  const [open, setOpen] = useState(false);
  const [input, setInput] = useState("");
  const [mode, setMode] = useState<CommandMode>("edit");
  const [phase, setPhase] = useState<Phase>({ type: "input" });
  const inputRef = useRef<HTMLInputElement>(null);

  const takeCommandBarInput = useAppStore((s) => s.takeCommandBarInput);

  const hasOntology = !!ontology;
  const canRefine =
    (activeProject?.status === "designed" || activeProject?.status === "completed") && commandStack.length === 0;
  const canEdit = hasOntology;

  // Auto-open from external triggers (Quality Panel "Ask AI", etc.)
  const handleEditSubmitRef = useRef<(() => void) | null>(null);
  useEffect(() => {
    const pending = takeCommandBarInput();
    if (pending && canEdit) {
      setMode("edit");
      setInput(pending);
      setOpen(true);
      // Auto-submit after opening
      setTimeout(() => handleEditSubmitRef.current?.(), 100);
    }
  }, [takeCommandBarInput, canEdit]);

  // Cmd+E to toggle the AI command bar. ⌘K is reserved for the
  // unified command palette (cross-app discrete commands); the
  // command bar is a natural-language prompt input — distinct UX,
  // distinct chord.
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "e") {
        e.preventDefault();
        // Allow opening if either mode is available
        if (!canEdit && !canRefine) return;
        setOpen((prev) => {
          if (!prev) {
            // Auto-select available mode
            if (!canEdit && canRefine) setMode("refine");
            else if (canEdit && !canRefine) setMode("edit");
            setTimeout(() => inputRef.current?.focus(), 50);
          } else {
            // Closing: reset phase
            setPhase({ type: "input" });
            setInput("");
          }
          return !prev;
        });
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [canEdit, canRefine]);

  // Reset phase when closing — derived from open state in close handler
  // (no effect needed, handleClose already resets phase)

  // ---------------------------------------------------------------------------
  // Edit mode submit: dry_run -> preview -> apply
  // ---------------------------------------------------------------------------

  const handleEditSubmit = useCallback(async () => {
    if (!activeProject || !input.trim()) return;

    setPhase({ type: "loading", message: t("loadingAnalyzing") });
    try {
      const resp = await editOntologyDraft(activeProject.id, {
        revision: activeProject.revision,
        user_request: input.trim(),
        dry_run: true,
      });

      if (resp.commands.length === 0) {
        toast.info(t("noChanges"), {
          description: resp.explanation || t("noChangesFallback"),
        });
        setPhase({ type: "input" });
        return;
      }

      setPhase({
        type: "preview",
        commands: resp.commands,
        explanation: resp.explanation,
      });
    } catch (err) {
      toast.error(t("editFailed"), {
        description: err instanceof Error ? err.message : t("toast.unknownError"),
      });
      setPhase({ type: "input" });
    }
  }, [activeProject, input, t]);

  // Render-phase ref sync — global keybindings invoke this without
  // a hook dep array. Switch to `useEffectEvent` when it leaves
  // experimental.
  handleEditSubmitRef.current = handleEditSubmit;

  const handleApplyCommands = useCallback(
    (accepted: OntologyCommand[]) => {
      for (const cmd of accepted) {
        applyCommand(cmd);
      }
      toast.success(t("toast.applied", { count: accepted.length }), {
        description: t("appliedDescription"),
      });
      setInput("");
      setOpen(false);
    },
    [applyCommand, t],
  );

  // ---------------------------------------------------------------------------
  // Refine mode submit (existing behavior)
  // ---------------------------------------------------------------------------

  const handleRefineSubmit = useCallback(async () => {
    if (!activeProject || !input.trim()) return;

    const confirmed = await confirmDialog({
      title: t("refineConfirmTitle"),
      description: t("refineConfirmDescription"),
      confirmLabel: t("refineConfirmLabel"),
      variant: "warning",
    });
    if (!confirmed) return;

    setPhase({ type: "loading", message: t("loadingRefining") });
    try {
      const resp = await refineOntologyDraft(activeProject.id, {
        revision: activeProject.revision,
        additional_context: input.trim(),
      });
      applyProjectSnapshot(resp.project);
      if (resp.reconcile_report) {
        setLastReconcileReport(resp.reconcile_report);
      }
      setInput("");
      setOpen(false);
      toast.success(t("refineSuccess"), { description: resp.profile_summary });
    } catch (err) {
      if (
        err instanceof ApiError &&
        err.code === "uncertain_reconcile" &&
        isPendingReconcile(err.params.details)
      ) {
        const details = err.params.details;
        setLastReconcileReport(details.report);
        useAppStore.getState().setPendingReconcile({
          report: details.report,
          reconciled_ontology: details.reconciled_ontology,
        });
        setInput("");
        setOpen(false);
        toast.warning(t("uncertainMatchesTitle"), {
          description: t("uncertainMatchesDescription", { count: details.report.uncertain_matches.length }),
          duration: TOAST_WARNING,
        });
      } else {
        toast.error(t("refineFailed"), {
          description: err instanceof Error ? err.message : t("toast.unknownError"),
        });
      }
      setPhase({ type: "input" });
    }
  }, [
    activeProject,
    input,
    confirmDialog,
    applyProjectSnapshot,
    setLastReconcileReport,
    t,
  ]);

  const handleSubmit = useCallback(() => {
    if (phase.type !== "input") return;
    if (mode === "edit") {
      handleEditSubmit();
    } else {
      handleRefineSubmit();
    }
  }, [mode, phase, handleEditSubmit, handleRefineSubmit]);

  const handleClose = useCallback(() => {
    if (phase.type === "loading") return; // don't close during loading
    setOpen(false);
    setInput("");
    setPhase({ type: "input" });
  }, [phase]);

  const handleCancelPreview = useCallback(() => {
    setPhase({ type: "input" });
  }, []);

  // Don't render if nothing can be done
  if (!canEdit && !canRefine) return null;

  const loading = phase.type === "loading";

  // Collapsed: small trigger button
  if (!open) {
    return (
      <div className="absolute bottom-4 start-1/2 z-canvas -translate-x-1/2">
        <button type="button"
          onClick={() => {
            if (!canEdit && canRefine) setMode("refine");
            else if (canEdit && !canRefine) setMode("edit");
            setPhase({ type: "input" });
            setOpen(true);
            setTimeout(() => inputRef.current?.focus(), 50);
          }}
          aria-expanded={open}
          className={cn(
            "flex items-center gap-2 rounded-full border border-divider bg-surface-base px-4 py-2 text-xs font-medium text-foreground shadow-3 backdrop-blur-sm transition-all duration-[var(--duration-base)] ease-[var(--ease-out)]",
            "hover:border-brand-border hover:bg-surface-base hover:text-brand-foreground hover:shadow-2",
          )}
        >
          <Wand2 className="h-3.5 w-3.5" />
          {t("askOntosyx")}
          <KeyboardShortcut keys="mod+k" size="default" className="ms-1" />
        </button>
      </div>
    );
  }

  // Expanded: command bar
  return (
    <div className="absolute bottom-4 start-1/2 z-canvas w-panel-wide -translate-x-1/2" role="dialog" aria-label={t("commandBarAria")}>
      {/* Preview panel (rendered above input when in preview phase) */}
      {phase.type === "preview" && (
        <div className="mb-2">
          <CommandPreview
            commands={phase.commands}
            explanation={phase.explanation}
            ontology={ontology}
            onApply={handleApplyCommands}
            onCancel={handleCancelPreview}
          />
        </div>
      )}

      {/* Main input panel */}
      <div
        className={cn(
          "rounded-xl border bg-surface-base shadow-4 backdrop-blur-sm",
          loading
            ? "border-brand-border"
            : "border-divider",
        )}
      >
        {/* Unsaved changes warning for refine mode */}
        {mode === "refine" && commandStack.length > 0 && (
          <div className="border-b border-warning-border bg-warning-surface px-4 py-1.5 text-2xs text-warning-foreground">
            {t("saveFirst")}
          </div>
        )}

        {/* Mode toggle + input row */}
        <div className="flex items-center gap-2 px-3 py-3">
          {/* Mode toggle */}
          <div className="flex shrink-0 rounded-lg border border-divider bg-surface-raised p-0.5">
            <button type="button"
              onClick={() => {
                if (canEdit) setMode("edit");
              }}
              disabled={!canEdit || loading}
              title={t("editTitle")}
              className={cn(
                "flex items-center gap-1 rounded-md px-2 py-1 text-2xs font-medium transition-all duration-[var(--duration-base)] ease-[var(--ease-out)]",
                mode === "edit"
                  ? "bg-surface-base text-foreground-strong shadow-1-strong"
                  : "text-foreground-muted hover:text-foreground-muted",
                (!canEdit || loading) && "cursor-not-allowed opacity-40",
              )}
            >
              <Pencil className="h-3 w-3" />
              {tCommon("edit")}
            </button>
            <button type="button"
              onClick={() => {
                if (canRefine) setMode("refine");
              }}
              disabled={!canRefine || loading}
              title={t("refineTitle")}
              className={cn(
                "flex items-center gap-1 rounded-md px-2 py-1 text-2xs font-medium transition-all duration-[var(--duration-base)] ease-[var(--ease-out)]",
                mode === "refine"
                  ? "bg-surface-base text-foreground-strong shadow-1-strong"
                  : "text-foreground-muted hover:text-foreground-muted",
                (!canRefine || loading) && "cursor-not-allowed opacity-40",
              )}
            >
              <Repeat className="h-3 w-3" />
              {t("refine")}
            </button>
          </div>

          {/* Loading spinner or wand icon */}
          {loading ? (
            <Spinner size="sm" className="shrink-0 text-brand-foreground" />
          ) : (
            <Wand2 className="h-4 w-4 shrink-0 text-foreground-muted" />
          )}

          {/* Input */}
          <input
            ref={inputRef}
            type="text"
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.shiftKey) {
                e.preventDefault();
                handleSubmit();
              }
              if (e.key === "Escape") {
                handleClose();
              }
            }}
            placeholder={
              mode === "edit"
                ? t("placeholderEdit")
                : t("placeholderRefine")
            }
            disabled={loading || phase.type === "preview"}
            aria-label={t("inputAria")}
            className={cn(
              "flex-1 bg-transparent text-sm text-foreground-strong outline-none placeholder:text-foreground-muted focus-visible:ring-2 focus-visible:ring-brand-foreground/40 focus-visible:rounded-md",
            )}
          />

          {/* Submit button */}
          {input.trim() && phase.type === "input" && (
            <button type="button"
              onClick={handleSubmit}
              disabled={mode === "refine" && !canRefine}
              className="flex items-center gap-1 rounded-lg bg-brand-solid px-3 py-1.5 text-xs font-medium text-foreground-onbrand hover:bg-brand-solid-hover disabled:opacity-50"
            >
              <Check className="h-3 w-3" />
              {mode === "edit" ? t("previewEdit") : t("refineBtn")}
            </button>
          )}

          {/* Close button */}
          <button type="button"
            onClick={handleClose}
            disabled={loading}
            aria-label={t("closeAria")}
            className="rounded-md p-1 text-foreground-muted hover:bg-surface-inset hover:text-foreground disabled:opacity-50"
          >
            <X className="h-3.5 w-3.5" />
          </button>
        </div>

        {/* Hint */}
        <div className="border-t border-divider-soft px-4 py-1.5">
          {loading && phase.type === "loading" ? (
            <LoadingHint baseMessage={phase.message} />
          ) : (
            <span className="text-2xs text-foreground-muted">
              {phase.type === "preview"
                ? t("hintPreview")
                : mode === "edit"
                  ? t("hintEdit")
                  : t("hintRefine")}
            </span>
          )}
        </div>
      </div>
    </div>
  );
}
