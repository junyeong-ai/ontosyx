"use client";

import { useCallback, useState } from "react";
import { useTranslations } from "next-intl";
import { useAppStore } from "@/lib/store";
import { editProject } from "@/lib/api";
import { defaultText } from "@/lib/locale/localize";
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
        return `Update description: "${defaultText(cmd.description).slice(0, 60)}..."`;
      case "update_edge_description":
        return `Update description: "${defaultText(cmd.description).slice(0, 60)}..."`;
      case "update_property":
        return `Update property: ${cmd.patch.description ? `description "${defaultText(cmd.patch.description).slice(0, 50)}..."` : JSON.stringify(cmd.patch)}`;
      case "batch":
        return `${cmd.description} (${cmd.commands.length} changes)`;
      default:
        return `${cmd.op}`;
    }
  })();

  return (
    <div className="flex items-center gap-1.5 border-b border-dashed border-concept-border bg-concept-surface px-3 py-1.5">
      <HugeiconsIcon icon={MagicWand01Icon} className="h-2.5 w-2.5 shrink-0 text-concept-foreground" size="100%" />
      <span className="min-w-0 flex-1 truncate text-concept-foreground">
        {label}
      </span>
      <Tooltip content="Accept">
        <button
          onClick={onAccept}
          aria-label="Accept"
          className="rounded p-0.5 text-brand-foreground hover:bg-brand-surface hover:text-brand-foreground dark:hover:bg-brand-surface"
        >
          <HugeiconsIcon icon={Tick01Icon} className="h-3 w-3" size="100%" />
        </button>
      </Tooltip>
      <Tooltip content="Reject">
        <button
          onClick={onReject}
          aria-label="Reject"
          className="rounded p-0.5 text-muted-foreground hover:bg-surface-inset hover:text-foreground"
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
  const t = useTranslations("inspector.toast");
  const applyCommand = useAppStore((s) => s.applyCommand);
  const [remaining, setRemaining] = useState(commands);

  const handleAccept = (idx: number) => {
    const cmd = remaining[idx];
    applyCommand(cmd);
    toast.success(t("suggestionApplied"));
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
    <div className="border-b border-concept-border">
      {explanation && (
        <p className="px-3 py-1 text-2xs text-concept-foreground">
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
            className="rounded bg-concept-foreground px-2 py-0.5 text-2xs font-medium text-white hover:bg-concept-foreground"
          >
            Accept All ({remaining.length})
          </button>
          <button
            onClick={onDismiss}
            className="rounded px-2 py-0.5 text-2xs text-muted-foreground hover:bg-surface-inset"
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
  const t = useTranslations("inspector.toast");
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
          toast.info(t("noSuggestions"));
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
    [activeProject, t],
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
        className="rounded p-0.5 text-concept-foreground hover:bg-concept-surface hover:text-concept-foreground disabled:opacity-50 dark:hover:bg-concept-surface"
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
