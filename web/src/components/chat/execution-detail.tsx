"use client";

import { useRouter } from "next/navigation";
import { useTranslations } from "next-intl";
import { useAppStore, type ChatMessage } from "@/lib/store";
import type { OntologyIR, QueryExecution } from "@/types/api";
import { getOntologyDetail } from "@/lib/api";
import { WidgetWithToolbar } from "@/components/widgets/widget-toolbar";
import { ResponseBasis } from "@/components/widgets/response-basis";
import { Button } from "@/components/ui/button";
import { HugeiconsIcon } from "@hugeicons/react";
import {
  ArrowLeft01Icon,
  PlayIcon,
  AiNetworkIcon,
} from "@hugeicons/core-free-icons";
import { toast } from "sonner";
import { useGuardPendingEdits } from "@/lib/guard-pending-edits";
import { arr } from "@/lib/ir-collections";

/**
 * Resolve the ontology IR for a past execution. Draft executions carry
 * an inline `ontology_snapshot`; committed executions reference an
 * identity uuid and the IR must be fetched via the detail endpoint.
 * Returns `null` when neither path yields an IR — the caller should
 * show a user-facing error rather than proceed.
 */
async function resolveExecutionOntology(
  execution: QueryExecution,
): Promise<OntologyIR | null> {
  if (execution.ontology_snapshot) return execution.ontology_snapshot;
  if (!execution.ontology_id) return null;
  try {
    const detail = await getOntologyDetail(execution.ontology_id);
    return detail.ontology_ir ?? null;
  } catch (err) {
    console.error("Failed to hydrate execution ontology:", err);
    return null;
  }
}

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
      <h3 className="mb-1.5 text-xs font-semibold uppercase tracking-wider text-muted-foreground">
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
  const t = useTranslations("workbench.chat.execution");
  const { setOntology, setActiveProject, addMessage, clearMessages, setHighlightedBindings } =
    useAppStore();
  const router = useRouter();
  const guardPendingEdits = useGuardPendingEdits();

  const handleLoadToChat = async () => {
    if (!(await guardPendingEdits(t("loadToChatGuardLabel")))) return;
    const ir = await resolveExecutionOntology(execution);
    if (!ir) {
      toast.error(t("loadOntologyFailed", { default: "Failed to load ontology" }));
      return;
    }
    // Detach from active project — loaded snapshot is standalone
    setActiveProject(null);
    setOntology(ir);
    clearMessages();

    const userMsg: ChatMessage = {
      id: crypto.randomUUID(),
      role: "user",
      content: execution.question,
    };
    addMessage(userMsg);
    router.push("/analyze");
  };

  const handleShowOnGraph = () => {
    if (!execution.query_bindings) return;
    const currentOntology = useAppStore.getState().ontology;
    if (!currentOntology) return;

    if (currentOntology.id !== execution.ontology_lineage_id) {
      toast.warning(t("differentOntologyWarning"), { duration: 5000 });
    }

    // Validate that referenced node/edge IDs exist in current ontology
    const bindings = execution.query_bindings;
    const currentNodeIds = new Set(arr(currentOntology.node_types).map((n: { id: string }) => n.id));
    const currentEdgeIds = new Set(arr(currentOntology.edge_types).map((e: { id: string }) => e.id));

    const validNodeBindings = bindings.node_bindings.filter(
      (b: { node_id: string }) => currentNodeIds.has(b.node_id),
    );
    const validEdgeBindings = bindings.edge_bindings.filter(
      (b: { edge_id: string }) => currentEdgeIds.has(b.edge_id),
    );

    const droppedNodes = bindings.node_bindings.length - validNodeBindings.length;
    const droppedEdges = bindings.edge_bindings.length - validEdgeBindings.length;

    if (droppedNodes > 0 || droppedEdges > 0) {
      toast.warning(t("bindingsDropped", { count: droppedNodes + droppedEdges }));
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
    if (!(await guardPendingEdits(t("showOnSnapshotGuardLabel")))) return;
    const ir = await resolveExecutionOntology(execution);
    if (!ir) {
      toast.error(t("loadOntologyFailed", { default: "Failed to load ontology" }));
      return;
    }
    // Detach from active project — viewing historical snapshot
    setActiveProject(null);
    setOntology(ir);
    setHighlightedBindings(execution.query_bindings);
  };

  const date = new Date(execution.created_at);

  return (
    <div className="flex h-full flex-col">
      {/* Header */}
      <div className="flex items-center gap-2 border-b border-zinc-200 px-4 py-3 dark:border-zinc-800">
        <Button variant="ghost" size="icon" onClick={onBack} aria-label={t("backAria")} className="shrink-0">
          <HugeiconsIcon icon={ArrowLeft01Icon} className="h-4 w-4" size="100%" />
        </Button>
        <div className="min-w-0 flex-1">
          <p className="truncate text-sm font-medium text-zinc-800 dark:text-zinc-200">
            {execution.question}
          </p>
          <p className="text-xs text-muted-foreground">
            {t("meta", {
              date: date.toLocaleString(),
              model: execution.model,
              duration: execution.execution_time_ms,
            })}
          </p>
        </div>
        <div className="flex shrink-0 items-center gap-2">
          {execution.query_bindings && (
            <>
              <Button variant="outline" size="sm" onClick={handleShowOnGraph} title={t("highlightTitle")}>
                <HugeiconsIcon icon={AiNetworkIcon} className="mr-1 h-3 w-3" size="100%" />
                {t("highlight")}
              </Button>
              <Button variant="outline" size="sm" onClick={handleShowOnSnapshot} title={t("replayTitle")}>
                <HugeiconsIcon icon={AiNetworkIcon} className="mr-1 h-3 w-3" size="100%" />
                {t("replay")}
              </Button>
            </>
          )}
          <Button variant="outline" size="sm" onClick={handleLoadToChat}>
            <HugeiconsIcon icon={PlayIcon} className="mr-1 h-3 w-3" size="100%" />
            {t("loadToChat")}
          </Button>
        </div>
      </div>

      {/* Body */}
      <div className="flex-1 space-y-4 overflow-y-auto p-4">
        {/* Explanation */}
        <Section title={t("sectionExplanation")}>
          <p className="text-sm leading-relaxed text-zinc-700 dark:text-zinc-300">
            {execution.explanation}
          </p>
        </Section>

        {/* Compiled query */}
        <Section title={t("sectionQueryTitle", { target: execution.compiled_target })}>
          <pre className="overflow-x-auto rounded-lg bg-zinc-900 p-3 text-xs text-emerald-400 dark:bg-zinc-950">
            {execution.compiled_query}
          </pre>
        </Section>

        {/* Results */}
        {execution.results && execution.results.rows.length > 0 && (
          <Section title={t("sectionResultsTitle", { count: execution.results.rows.length })}>
            <div className="space-y-3">
              <WidgetWithToolbar
                spec={(execution.widget as Record<string, unknown>) ?? { widget: "auto" }}
                data={execution.results}
              />
              <ResponseBasis provenance={execution.results.metadata?.provenance} warnings={execution.results.metadata?.warnings} />
            </div>
          </Section>
        )}

        {/* Ontology info */}
        <Section title={t("sectionOntology")}>
          <p className="text-xs text-muted-foreground">
            {t("ontologyMeta", {
              id: execution.ontology_lineage_id,
              version: execution.ontology_version,
            })}
          </p>
        </Section>
      </div>
    </div>
  );
}
