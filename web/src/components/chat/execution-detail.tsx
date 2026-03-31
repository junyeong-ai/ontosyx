"use client";

import { useAppStore, type ChatMessage } from "@/lib/store";
import type { QueryExecution } from "@/types/api";
import { WidgetWithToolbar } from "@/components/widgets/widget-toolbar";
import { Button } from "@/components/ui/button";
import { HugeiconsIcon } from "@hugeicons/react";
import {
  ArrowLeft01Icon,
  PlayIcon,
  AiNetworkIcon,
} from "@hugeicons/core-free-icons";
import { toast } from "sonner";
import { useGuardPendingEdits } from "@/lib/guard-pending-edits";

// ---------------------------------------------------------------------------
// Section — reusable collapsible section header
// ---------------------------------------------------------------------------

export interface SectionProps {
  title: string;
  children: React.ReactNode;
}

export function Section({ title, children }: SectionProps) {
  return (
    <div>
      <h3 className="mb-1.5 text-xs font-semibold uppercase tracking-wider text-zinc-400">
        {title}
      </h3>
      {children}
    </div>
  );
}

// ---------------------------------------------------------------------------
// ExecutionDetail — full query execution view with actions
// ---------------------------------------------------------------------------

export interface ExecutionDetailProps {
  execution: QueryExecution;
  onBack: () => void;
}

export function ExecutionDetail({ execution, onBack }: ExecutionDetailProps) {
  const { setOntology, setActiveProject, setWorkspaceMode, addMessage, clearMessages, setHighlightedBindings } =
    useAppStore();
  const guardPendingEdits = useGuardPendingEdits();

  const handleLoadToChat = async () => {
    if (!(await guardPendingEdits("Load to Chat"))) return;
    // Detach from active project — loaded snapshot is standalone
    setActiveProject(null);
    setOntology(execution.ontology_snapshot);
    clearMessages();

    const userMsg: ChatMessage = {
      id: crypto.randomUUID(),
      role: "user",
      content: execution.question,
    };
    addMessage(userMsg);
    setWorkspaceMode("analyze");
  };

  const handleShowOnGraph = () => {
    if (!execution.query_bindings) return;
    const currentOntology = useAppStore.getState().ontology;
    if (!currentOntology) return;

    if (currentOntology.id !== execution.ontology_id) {
      toast.warning(
        "This query was executed on a different ontology version. Highlights may not match the current graph.",
        { duration: 5000 },
      );
    }

    // Validate that referenced node/edge IDs exist in current ontology
    const bindings = execution.query_bindings;
    const currentNodeIds = new Set(currentOntology.node_types.map((n: { id: string }) => n.id));
    const currentEdgeIds = new Set(currentOntology.edge_types.map((e: { id: string }) => e.id));

    const validNodeBindings = bindings.node_bindings.filter(
      (b: { node_id: string }) => currentNodeIds.has(b.node_id),
    );
    const validEdgeBindings = bindings.edge_bindings.filter(
      (b: { edge_id: string }) => currentEdgeIds.has(b.edge_id),
    );

    const droppedNodes = bindings.node_bindings.length - validNodeBindings.length;
    const droppedEdges = bindings.edge_bindings.length - validEdgeBindings.length;

    if (droppedNodes > 0 || droppedEdges > 0) {
      toast.warning(
        `${droppedNodes + droppedEdges} binding(s) could not be resolved against the current ontology and were skipped.`,
      );
    }

    setHighlightedBindings({
      ...bindings,
      node_bindings: validNodeBindings,
      edge_bindings: validEdgeBindings,
    });
  };

  /** Deterministic replay: switch to the execution's ontology snapshot and highlight all bindings exactly */
  const handleShowOnSnapshot = async () => {
    if (!execution.query_bindings) return;
    if (!(await guardPendingEdits("Show on Snapshot"))) return;
    // Detach from active project — viewing historical snapshot
    setActiveProject(null);
    setOntology(execution.ontology_snapshot);
    setHighlightedBindings(execution.query_bindings);
  };

  const date = new Date(execution.created_at);

  return (
    <div className="flex h-full flex-col">
      {/* Header */}
      <div className="flex items-center gap-2 border-b border-zinc-200 px-4 py-3 dark:border-zinc-800">
        <Button variant="ghost" size="icon" onClick={onBack} aria-label="Back" className="shrink-0">
          <HugeiconsIcon icon={ArrowLeft01Icon} className="h-4 w-4" size="100%" />
        </Button>
        <div className="min-w-0 flex-1">
          <p className="truncate text-sm font-medium text-zinc-800 dark:text-zinc-200">
            {execution.question}
          </p>
          <p className="text-xs text-zinc-400">
            {date.toLocaleString()} &middot; {execution.model} &middot;{" "}
            {execution.execution_time_ms}ms
          </p>
        </div>
        <div className="flex shrink-0 items-center gap-2">
          {execution.query_bindings && (
            <>
              <Button variant="outline" size="sm" onClick={handleShowOnGraph} title="Highlight on current ontology (best-effort)">
                <HugeiconsIcon icon={AiNetworkIcon} className="mr-1 h-3 w-3" size="100%" />
                Highlight
              </Button>
              <Button variant="outline" size="sm" onClick={handleShowOnSnapshot} title="Switch to execution's ontology snapshot for exact replay">
                <HugeiconsIcon icon={AiNetworkIcon} className="mr-1 h-3 w-3" size="100%" />
                Replay
              </Button>
            </>
          )}
          <Button variant="outline" size="sm" onClick={handleLoadToChat}>
            <HugeiconsIcon icon={PlayIcon} className="mr-1 h-3 w-3" size="100%" />
            Load to chat
          </Button>
        </div>
      </div>

      {/* Body */}
      <div className="flex-1 space-y-4 overflow-y-auto p-4">
        {/* Explanation */}
        <Section title="Explanation">
          <p className="text-sm leading-relaxed text-zinc-700 dark:text-zinc-300">
            {execution.explanation}
          </p>
        </Section>

        {/* Compiled query */}
        <Section title={`${execution.compiled_target} query`}>
          <pre className="overflow-x-auto rounded-lg bg-zinc-900 p-3 text-xs text-emerald-400 dark:bg-zinc-950">
            {execution.compiled_query}
          </pre>
        </Section>

        {/* Results */}
        {execution.results && execution.results.rows.length > 0 && (
          <Section title={`Results (${execution.results.rows.length} rows)`}>
            <WidgetWithToolbar
              spec={(execution.widget as Record<string, unknown>) ?? { widget: "auto" }}
              data={execution.results}
            />
          </Section>
        )}

        {/* Ontology info */}
        <Section title="Ontology">
          <p className="text-xs text-zinc-500">
            {execution.ontology_id} v{execution.ontology_version}
          </p>
        </Section>
      </div>
    </div>
  );
}
