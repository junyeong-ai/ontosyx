/* eslint-disable react-hooks/rules-of-hooks */
"use client";

import { useCallback, useEffect, useRef, useState } from "react";
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
import type {
  OntologyIR,
  OntologyCommand,
} from "@/types/api";

// Re-export extracted components for backward compatibility
export { DiffOverlayBar } from "./diff-overlay-bar";
export { VersionDiffBar } from "./version-diff-bar";

// ---------------------------------------------------------------------------
// Loading hints — rotate tips while LLM processes
// ---------------------------------------------------------------------------

const LOADING_TIPS = [
  // Edit tips
  "Be specific — 'Add string property email to Customer node'",
  "Include data type and target — 'Add integer property age to Driver'",
  "You can remove edges too — 'Remove the MANAGES edge'",
  "Batch edits work — 'Add description to all nodes missing one'",
  // Undo & safety
  "Every edit can be undone with Ctrl+Z",
  "Edit mode previews changes before applying — review carefully",
  "Refine mode replaces the entire ontology — use Edit for small changes",
  // Workflow tips
  "Quality tab shows gaps the AI can auto-fix with one click",
  "Click a quality gap's Fix button to auto-generate the edit request",
  "Use the Chat tab for multi-step analysis and complex questions",
  // Schema & data tips
  "The agent can explore your source schema — ask about table structures",
  "Ask 'detect schema drift' to find mismatches between source and ontology",
  "Use recall_memory to find similar past analyses across sessions",
  // Analysis tips
  "Prefix with ! in Chat for raw Cypher queries — skip the AI",
  "Pin query results to dashboards directly from the Chat",
  "Ask for EDA to get automated distribution and outlier analysis",
  // Design best practices
  "Good node names are singular nouns — Customer, not Customers",
  "Every node should have a description for better query translation",
  "Unique constraints help the AI generate more precise queries",
];

function LoadingHint({ baseMessage }: { baseMessage: string }) {
  const [tipIndex, setTipIndex] = useState(0);
  const [showTip, setShowTip] = useState(false);

  useEffect(() => {
    // Show first tip after 2 seconds
    const showTimer = setTimeout(() => setShowTip(true), 2000);
    // Rotate tips every 3 seconds
    const rotateTimer = setInterval(() => {
      setTipIndex((prev) => (prev + 1) % LOADING_TIPS.length);
    }, 3000);
    return () => {
      clearTimeout(showTimer);
      clearInterval(rotateTimer);
    };
  }, []);

  return (
    <div className="flex items-center gap-1.5">
      <span className="text-[10px] text-zinc-400">{baseMessage}</span>
      {showTip && (
        <span className="text-[9px] text-zinc-400/50 transition-opacity duration-300">
          · {LOADING_TIPS[tipIndex]}
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
  const activeProject = useAppStore((s) => s.activeProject);
  const setActiveProject = useAppStore((s) => s.setActiveProject);
  const setOntology = useAppStore((s) => s.setOntology);
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

    setPhase({ type: "loading", message: "Analyzing structure & generating commands..." });
    try {
      const resp = await editProject(activeProject.id, {
        revision: activeProject.revision,
        user_request: input.trim(),
        dry_run: true,
      });

      if (resp.commands.length === 0) {
        toast.info("No changes needed", {
          description: resp.explanation || "The ontology already matches your request.",
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
      toast.error("Edit failed", {
        description: err instanceof Error ? err.message : "Unknown error",
      });
      setPhase({ type: "input" });
    }
  }, [activeProject, input]);

  // Keep ref in sync for auto-submit from external triggers
  handleEditSubmitRef.current = handleEditSubmit;

  const handleApplyCommands = useCallback(
    (accepted: OntologyCommand[]) => {
      for (const cmd of accepted) {
        applyCommand(cmd);
      }
      toast.success(`Applied ${accepted.length} command${accepted.length === 1 ? "" : "s"}`, {
        description: "Use Ctrl+Z to undo individual commands",
      });
      setInput("");
      setOpen(false);
    },
    [applyCommand],
  );

  // ---------------------------------------------------------------------------
  // Refine mode submit (existing behavior)
  // ---------------------------------------------------------------------------

  const handleRefineSubmit = useCallback(async () => {
    if (!activeProject || !input.trim()) return;

    const confirmed = await confirmDialog({
      title: "Refine Ontology",
      description:
        "Refine will replace the entire ontology with a new LLM-generated version. This cannot be undone. Continue?",
      confirmLabel: "Refine",
      variant: "warning",
    });
    if (!confirmed) return;

    setPhase({ type: "loading", message: "Refining ontology with additional context..." });
    try {
      const resp = await refineProject(activeProject.id, {
        revision: activeProject.revision,
        additional_context: input.trim(),
      });
      setActiveProject(resp.project);
      if (resp.project.ontology) {
        setOntology(resp.project.ontology as OntologyIR);
      }
      if (resp.reconcile_report) {
        setLastReconcileReport(resp.reconcile_report);
      }
      setInput("");
      setOpen(false);
      toast.success("Ontology refined", { description: resp.profile_summary });
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
        toast.warning("Refine produced uncertain matches", {
          description: `${details.report.uncertain_matches.length} match(es) need review`,
          duration: 8000,
        });
      } else {
        toast.error("Refine failed", {
          description: err instanceof Error ? err.message : "Unknown error",
        });
      }
      setPhase({ type: "input" });
    }
  }, [
    activeProject,
    input,
    confirmDialog,
    setActiveProject,
    setOntology,
    setLastReconcileReport,
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
            "dark:border-zinc-700 dark:bg-zinc-900/90 dark:text-zinc-400 dark:hover:border-emerald-600 dark:hover:text-emerald-400",
          )}
        >
          <HugeiconsIcon
            icon={MagicWand01Icon}
            className="h-3.5 w-3.5"
            size="100%"
          />
          Ask Ontosyx
          <kbd className="ml-1 rounded bg-zinc-100 px-1.5 py-0.5 text-[9px] font-mono text-zinc-400 dark:bg-zinc-800">
            {"\u2318"}K
          </kbd>
        </button>
      </div>
    );
  }

  // Expanded: command bar
  return (
    <div className="absolute bottom-4 left-1/2 z-10 w-[560px] -translate-x-1/2" role="dialog" aria-label="Command bar">
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
            Save pending changes first ({"\u2318"}S) before using LLM refine
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
              title="Surgical edits — preview commands before applying, full undo support"
              className={cn(
                "flex items-center gap-1 rounded-md px-2 py-1 text-[10px] font-medium transition-all",
                mode === "edit"
                  ? "bg-white text-zinc-800 shadow-sm dark:bg-zinc-700 dark:text-zinc-200"
                  : "text-zinc-400 hover:text-zinc-600 dark:hover:text-zinc-300",
                (!canEdit || loading) && "cursor-not-allowed opacity-40",
              )}
            >
              <HugeiconsIcon
                icon={Edit01Icon}
                className="h-3 w-3"
                size="100%"
              />
              Edit
            </button>
            <button
              onClick={() => {
                if (canRefine) setMode("refine");
              }}
              disabled={!canRefine || loading}
              title="Full LLM redesign — replaces the entire ontology"
              className={cn(
                "flex items-center gap-1 rounded-md px-2 py-1 text-[10px] font-medium transition-all",
                mode === "refine"
                  ? "bg-white text-zinc-800 shadow-sm dark:bg-zinc-700 dark:text-zinc-200"
                  : "text-zinc-400 hover:text-zinc-600 dark:hover:text-zinc-300",
                (!canRefine || loading) && "cursor-not-allowed opacity-40",
              )}
            >
              <HugeiconsIcon
                icon={RepeatIcon}
                className="h-3 w-3"
                size="100%"
              />
              Refine
            </button>
          </div>

          {/* Loading spinner or wand icon */}
          {loading ? (
            <Spinner size="sm" className="shrink-0 text-emerald-500" />
          ) : (
            <HugeiconsIcon
              icon={MagicWand01Icon}
              className="h-4 w-4 shrink-0 text-zinc-400"
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
                ? "Describe changes... e.g. 'Add a Category node with name property'"
                : "Describe ontology refinement... (full LLM redesign)"
            }
            disabled={loading || phase.type === "preview"}
            aria-label="Enter command"
            className={cn(
              "flex-1 bg-transparent text-sm text-zinc-800 outline-none placeholder:text-zinc-500",
              "dark:text-zinc-200 dark:placeholder:text-zinc-500",
            )}
          />

          {/* Submit button */}
          {input.trim() && phase.type === "input" && (
            <button
              onClick={handleSubmit}
              disabled={mode === "refine" && !canRefine}
              className="flex items-center gap-1 rounded-lg bg-emerald-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-emerald-700 disabled:opacity-50"
            >
              <HugeiconsIcon
                icon={Tick01Icon}
                className="h-3 w-3"
                size="100%"
              />
              {mode === "edit" ? "Preview" : "Refine"}
            </button>
          )}

          {/* Close button */}
          <button
            onClick={handleClose}
            disabled={loading}
            aria-label="Close command bar"
            className="rounded-md p-1 text-zinc-400 hover:bg-zinc-100 hover:text-zinc-600 disabled:opacity-50 dark:hover:bg-zinc-800"
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
            <span className="text-[10px] text-zinc-400">
              {phase.type === "preview"
                ? "Select commands to apply, then click Apply"
                : mode === "edit"
                  ? "Enter to preview commands \u00b7 Esc to close \u00b7 Surgical edits with undo support"
                  : "Enter to submit \u00b7 Esc to close \u00b7 Full LLM redesign (replaces ontology)"}
            </span>
          )}
        </div>
      </div>
    </div>
  );
}
