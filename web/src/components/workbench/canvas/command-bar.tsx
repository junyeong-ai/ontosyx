"use client";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslations } from "next-intl";
import { useAppStore } from "@/lib/store";
import {
  ApiError,
  isPendingReconcile,
  refineProject,
  editProject,
} from "@/lib/api";
import { HugeiconsIcon } from "@hugeicons/react";
import {
  MagicWand01Icon,
  Cancel01Icon,
  Tick01Icon,
  Edit01Icon,
  RepeatIcon,
} from "@hugeicons/core-free-icons";
import { Spinner } from "@/components/ui/spinner";
import { CommandPreview } from "./command-preview";
import { cn } from "@/lib/cn";
import { toast } from "sonner";
import { useConfirm } from "@/components/ui/confirm-dialog";
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
      <span className="text-[10px] text-muted-foreground">{baseMessage}</span>
      {showTip && (
        <span className="text-[9px] text-muted-foreground/50 transition-opacity duration-300">
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

  // Cmd+K to toggle command bar
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === "k") {
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
  }, [canEdit, canRefine, setMode, setPhase, setInput]);

  // Reset phase when closing — derived from open state in close handler
  // (no effect needed, handleClose already resets phase)

  // ---------------------------------------------------------------------------
  // Edit mode submit: dry_run -> preview -> apply
  // ---------------------------------------------------------------------------

  const handleEditSubmit = useCallback(async () => {
    if (!activeProject || !input.trim()) return;

    setPhase({ type: "loading", message: t("loadingAnalyzing") });
    try {
      const resp = await editProject(activeProject.id, {
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

  // Keep ref in sync for auto-submit from external triggers.
  // TODO(phase-2): replace this render-phase assignment with
  // `useEffectEvent(handleEditSubmit)` once a stable API is available;
  // until then the guarded assignment is the minimum-risk way to
  // expose the latest callback to a global keybinding.
  // eslint-disable-next-line react-hooks/refs
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
      const resp = await refineProject(activeProject.id, {
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
        err.type === "uncertain_reconcile" &&
        isPendingReconcile(err.details)
      ) {
        const details = err.details;
        setLastReconcileReport(details.report);
        useAppStore.getState().setPendingReconcile({
          report: details.report,
          reconciled_ontology: details.reconciled_ontology,
        });
        setInput("");
        setOpen(false);
        toast.warning(t("uncertainMatchesTitle"), {
          description: t("uncertainMatchesDescription", { count: details.report.uncertain_matches.length }),
          duration: 8000,
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
      <div className="absolute bottom-4 left-1/2 z-10 -translate-x-1/2">
        <button
          onClick={() => {
            if (!canEdit && canRefine) setMode("refine");
            else if (canEdit && !canRefine) setMode("edit");
            setPhase({ type: "input" });
            setOpen(true);
            setTimeout(() => inputRef.current?.focus(), 50);
          }}
          aria-expanded={open}
          className={cn(
            "flex items-center gap-2 rounded-full border border-zinc-200 bg-white/90 px-4 py-2 text-xs font-medium text-zinc-600 shadow-lg backdrop-blur-sm transition-all",
            "hover:border-emerald-300 hover:bg-white hover:text-emerald-700 hover:shadow-emerald-100",
            "dark:border-zinc-700 dark:bg-zinc-900/90 dark:text-muted-foreground dark:hover:border-emerald-600 dark:hover:text-emerald-400",
          )}
        >
          <HugeiconsIcon
            icon={MagicWand01Icon}
            className="h-3.5 w-3.5"
            size="100%"
          />
          {t("askOntosyx")}
          <kbd className="ml-1 rounded bg-zinc-100 px-1.5 py-0.5 text-[9px] font-mono text-muted-foreground dark:bg-zinc-800">
            {"\u2318"}K
          </kbd>
        </button>
      </div>
    );
  }

  // Expanded: command bar
  return (
    <div className="absolute bottom-4 left-1/2 z-10 w-[560px] -translate-x-1/2" role="dialog" aria-label={t("commandBarAria")}>
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
          "rounded-xl border bg-white/95 shadow-2xl backdrop-blur-sm",
          "dark:border-zinc-700 dark:bg-zinc-900/95",
          loading
            ? "border-emerald-300 dark:border-emerald-700"
            : "border-zinc-200",
        )}
      >
        {/* Unsaved changes warning for refine mode */}
        {mode === "refine" && commandStack.length > 0 && (
          <div className="border-b border-amber-200 bg-amber-50 px-4 py-1.5 text-[10px] text-amber-700 dark:border-amber-900 dark:bg-amber-950/30 dark:text-amber-400">
            {t("saveFirst")}
          </div>
        )}

        {/* Mode toggle + input row */}
        <div className="flex items-center gap-2 px-3 py-3">
          {/* Mode toggle */}
          <div className="flex shrink-0 rounded-lg border border-zinc-200 bg-zinc-50 p-0.5 dark:border-zinc-700 dark:bg-zinc-800">
            <button
              onClick={() => {
                if (canEdit) setMode("edit");
              }}
              disabled={!canEdit || loading}
              title={t("editTitle")}
              className={cn(
                "flex items-center gap-1 rounded-md px-2 py-1 text-[10px] font-medium transition-all",
                mode === "edit"
                  ? "bg-white text-zinc-800 shadow-sm dark:bg-zinc-700 dark:text-zinc-200"
                  : "text-muted-foreground hover:text-zinc-600 dark:hover:text-zinc-300",
                (!canEdit || loading) && "cursor-not-allowed opacity-40",
              )}
            >
              <HugeiconsIcon
                icon={Edit01Icon}
                className="h-3 w-3"
                size="100%"
              />
              {tCommon("edit")}
            </button>
            <button
              onClick={() => {
                if (canRefine) setMode("refine");
              }}
              disabled={!canRefine || loading}
              title={t("refineTitle")}
              className={cn(
                "flex items-center gap-1 rounded-md px-2 py-1 text-[10px] font-medium transition-all",
                mode === "refine"
                  ? "bg-white text-zinc-800 shadow-sm dark:bg-zinc-700 dark:text-zinc-200"
                  : "text-muted-foreground hover:text-zinc-600 dark:hover:text-zinc-300",
                (!canRefine || loading) && "cursor-not-allowed opacity-40",
              )}
            >
              <HugeiconsIcon
                icon={RepeatIcon}
                className="h-3 w-3"
                size="100%"
              />
              {t("refine")}
            </button>
          </div>

          {/* Loading spinner or wand icon */}
          {loading ? (
            <Spinner size="sm" className="shrink-0 text-emerald-500" />
          ) : (
            <HugeiconsIcon
              icon={MagicWand01Icon}
              className="h-4 w-4 shrink-0 text-muted-foreground"
              size="100%"
            />
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
              "flex-1 bg-transparent text-sm text-zinc-800 outline-none placeholder:text-muted-foreground",
              "dark:text-zinc-200 dark:placeholder:text-zinc-500",
            )}
          />

          {/* Submit button */}
          {input.trim() && phase.type === "input" && (
            <button
              onClick={handleSubmit}
              disabled={mode === "refine" && !canRefine}
              className="flex items-center gap-1 rounded-lg bg-emerald-700 px-3 py-1.5 text-xs font-medium text-white hover:bg-emerald-800 disabled:opacity-50"
            >
              <HugeiconsIcon
                icon={Tick01Icon}
                className="h-3 w-3"
                size="100%"
              />
              {mode === "edit" ? t("previewEdit") : t("refineBtn")}
            </button>
          )}

          {/* Close button */}
          <button
            onClick={handleClose}
            disabled={loading}
            aria-label={t("closeAria")}
            className="rounded-md p-1 text-muted-foreground hover:bg-zinc-100 hover:text-zinc-600 disabled:opacity-50 dark:hover:bg-zinc-800"
          >
            <HugeiconsIcon
              icon={Cancel01Icon}
              className="h-3.5 w-3.5"
              size="100%"
            />
          </button>
        </div>

        {/* Hint */}
        <div className="border-t border-zinc-100 px-4 py-1.5 dark:border-zinc-800">
          {loading && phase.type === "loading" ? (
            <LoadingHint baseMessage={phase.message} />
          ) : (
            <span className="text-[10px] text-muted-foreground">
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
