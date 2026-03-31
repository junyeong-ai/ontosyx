"use client";

import { useCallback, useState } from "react";
import { useAppStore } from "@/lib/store";
import { editProject } from "@/lib/api";
import { HugeiconsIcon } from "@hugeicons/react";
import {
  Tick01Icon,
  Cancel01Icon,
  MagicWand01Icon,
} from "@hugeicons/core-free-icons";
import { toast } from "sonner";
import { Tooltip } from "@/components/ui/tooltip";
import { Spinner } from "@/components/ui/spinner";
import type { OntologyCommand } from "@/types/api";
import { formatPropertyType } from "@/types/api";

// ---------------------------------------------------------------------------
// AI suggestion row (accept/reject for a single command)
// ---------------------------------------------------------------------------

export function AiSuggestionRow({
  cmd,
  onAccept,
  onReject,
}: {
  cmd: OntologyCommand;
  onAccept: () => void;
  onReject: () => void;
}) {
  const label = (() => {
    switch (cmd.op) {
      case "add_property":
        return `Add property "${cmd.property.name}" (${formatPropertyType(cmd.property.property_type)})`;
      case "add_node":
        return `Add node "${cmd.label}"`;
      case "add_edge":
        return `Add edge "${cmd.label}"`;
      case "update_node_description":
        return `Update description: "${cmd.description?.slice(0, 60) ?? ""}..."`;
      case "update_edge_description":
        return `Update description: "${cmd.description?.slice(0, 60) ?? ""}..."`;
      case "update_property":
        return `Update property: ${cmd.patch.description ? `description "${cmd.patch.description.slice(0, 50)}..."` : JSON.stringify(cmd.patch)}`;
      case "batch":
        return `${cmd.description} (${cmd.commands.length} changes)`;
      default:
        return `${cmd.op}`;
    }
  })();

  return (
    <div className="flex items-center gap-1.5 border-b border-dashed border-violet-200 bg-violet-50/40 px-3 py-1.5 dark:border-violet-800 dark:bg-violet-950/20">
      <HugeiconsIcon icon={MagicWand01Icon} className="h-2.5 w-2.5 shrink-0 text-violet-400" size="100%" />
      <span className="min-w-0 flex-1 truncate text-violet-700 dark:text-violet-300">
        {label}
      </span>
      <Tooltip content="Accept">
        <button
          onClick={onAccept}
          aria-label="Accept"
          className="rounded p-0.5 text-emerald-500 hover:bg-emerald-50 hover:text-emerald-600 dark:hover:bg-emerald-950"
        >
          <HugeiconsIcon icon={Tick01Icon} className="h-3 w-3" size="100%" />
        </button>
      </Tooltip>
      <Tooltip content="Reject">
        <button
          onClick={onReject}
          aria-label="Reject"
          className="rounded p-0.5 text-zinc-400 hover:bg-zinc-100 hover:text-zinc-600 dark:hover:bg-zinc-800"
        >
          <HugeiconsIcon icon={Cancel01Icon} className="h-3 w-3" size="100%" />
        </button>
      </Tooltip>
    </div>
  );
}

// ---------------------------------------------------------------------------
// AI suggestion list (shown inline after a dry-run response)
// ---------------------------------------------------------------------------

export function AiSuggestionList({
  commands,
  explanation,
  onDismiss,
}: {
  commands: OntologyCommand[];
  explanation: string;
  onDismiss: () => void;
}) {
  const applyCommand = useAppStore((s) => s.applyCommand);
  const [remaining, setRemaining] = useState(commands);

  const handleAccept = (idx: number) => {
    const cmd = remaining[idx];
    applyCommand(cmd);
    toast.success("Suggestion applied");
    const next = remaining.filter((_, i) => i !== idx);
    setRemaining(next);
    if (next.length === 0) onDismiss();
  };

  const handleReject = (idx: number) => {
    const next = remaining.filter((_, i) => i !== idx);
    setRemaining(next);
    if (next.length === 0) onDismiss();
  };

  const handleAcceptAll = () => {
    for (const cmd of remaining) {
      applyCommand(cmd);
    }
    toast.success(`${remaining.length} suggestion(s) applied`);
    onDismiss();
  };

  if (remaining.length === 0) return null;

  return (
    <div className="border-b border-violet-200 dark:border-violet-800">
      {explanation && (
        <p className="px-3 py-1 text-[10px] text-violet-500 dark:text-violet-400">
          {explanation}
        </p>
      )}
      {remaining.map((cmd, i) => (
        <AiSuggestionRow
          key={`${cmd.op}-${JSON.stringify(cmd).slice(0, 80)}`}
          cmd={cmd}
          onAccept={() => handleAccept(i)}
          onReject={() => handleReject(i)}
        />
      ))}
      {remaining.length > 1 && (
        <div className="flex items-center gap-1.5 px-3 py-1">
          <button
            onClick={handleAcceptAll}
            className="rounded bg-violet-600 px-2 py-0.5 text-[10px] font-medium text-white hover:bg-violet-700"
          >
            Accept All ({remaining.length})
          </button>
          <button
            onClick={onDismiss}
            className="rounded px-2 py-0.5 text-[10px] text-zinc-500 hover:bg-zinc-100 dark:hover:bg-zinc-800"
          >
            Dismiss
          </button>
        </div>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// useAiEdit hook — shared logic for AI edit dry-run
// ---------------------------------------------------------------------------

export function useAiEdit() {
  const activeProject = useAppStore((s) => s.activeProject);
  const ontology = useAppStore((s) => s.ontology);
  const [loading, setLoading] = useState(false);
  const [suggestions, setSuggestions] = useState<{
    commands: OntologyCommand[];
    explanation: string;
  } | null>(null);

  const canEdit = !!activeProject && !!ontology;

  const requestEdit = useCallback(
    async (userRequest: string) => {
      if (!activeProject) return;
      setLoading(true);
      try {
        const resp = await editProject(activeProject.id, {
          revision: activeProject.revision,
          user_request: userRequest,
          dry_run: true,
        });
        if (resp.commands.length === 0) {
          toast.info("AI found no changes to suggest");
        } else {
          setSuggestions({
            commands: resp.commands,
            explanation: resp.explanation,
          });
        }
      } catch (err) {
        toast.error(err instanceof Error ? err.message : "AI edit failed");
      } finally {
        setLoading(false);
      }
    },
    [activeProject],
  );

  const dismiss = useCallback(() => setSuggestions(null), []);

  return { canEdit, loading, suggestions, requestEdit, dismiss, ontology };
}

// ---------------------------------------------------------------------------
// AI Assist button (small icon button for section headers)
// ---------------------------------------------------------------------------

export function AiAssistButton({
  tooltip,
  loading,
  onClick,
}: {
  tooltip: string;
  loading: boolean;
  onClick: () => void;
}) {
  return (
    <Tooltip content={tooltip}>
      <button
        onClick={onClick}
        disabled={loading}
        aria-label={tooltip}
        className="rounded p-0.5 text-violet-400 hover:bg-violet-50 hover:text-violet-600 disabled:opacity-50 dark:hover:bg-violet-950"
      >
        {loading ? (
          <Spinner size="xs" />
        ) : (
          <HugeiconsIcon icon={MagicWand01Icon} className="h-3 w-3" size="100%" />
        )}
      </button>
    </Tooltip>
  );
}
