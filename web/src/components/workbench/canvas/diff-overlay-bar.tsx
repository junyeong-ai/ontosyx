"use client";

import { useEffect, useState } from "react";
import { useAppStore } from "@/lib/store";
import { applyReconcile } from "@/lib/api";
import { cn } from "@/lib/cn";
import { toast } from "sonner";
import type {
  OntologyIR,
  ReconcileReport,
  MatchDecision,
} from "@/types/api";

// ---------------------------------------------------------------------------
// Diff overlay dismiss bar -- shown when reconcile report is active
// ---------------------------------------------------------------------------

export function DiffOverlayBar() {
  const report = useAppStore((s) => s.lastReconcileReport);
  const setReport = useAppStore((s) => s.setLastReconcileReport);
  const pending = useAppStore((s) => s.pendingReconcile);
  const setPending = useAppStore((s) => s.setPendingReconcile);
  const activeProject = useAppStore((s) => s.activeProject);
  const setActiveProject = useAppStore((s) => s.setActiveProject);
  const setOntology = useAppStore((s) => s.setOntology);

  const [decisions, setDecisions] = useState<Record<string, boolean>>({});
  const [applying, setApplying] = useState(false);
  const [expanded, setExpanded] = useState(false);

  // Initialize decisions when pending reconcile changes (default: all accepted)
  useEffect(() => {
    if (pending) {
      const initial: Record<string, boolean> = {};
      for (const m of pending.report.uncertain_matches) {
        initial[m.original_id] = true; // default: accept
      }
      setDecisions(initial);
      setExpanded(true);
    } else {
      setDecisions({});
      setExpanded(false);
    }
  }, [pending]);

  if (!report) return null;

  const addedCount = report.generated_ids.length;
  const uncertainCount = report.uncertain_matches.length;
  const deletedCount = report.deleted_entities.length;

  const handleDismiss = () => {
    setReport(null);
    setPending(null);
  };

  const toggleDecision = (originalId: string) => {
    setDecisions((prev) => ({ ...prev, [originalId]: !prev[originalId] }));
  };

  const handleApplyDecisions = async () => {
    if (!pending || !activeProject) return;
    setApplying(true);
    try {
      const matchDecisions: MatchDecision[] =
        pending.report.uncertain_matches.map((m) => ({
          original_id: m.original_id,
          accept: decisions[m.original_id] ?? true,
        }));
      const resp = await applyReconcile(activeProject.id, {
        revision: activeProject.revision,
        reconciled_ontology: pending.reconciled_ontology,
        decisions: matchDecisions,
        uncertain_matches: pending.report.uncertain_matches,
      });
      setActiveProject(resp.project);
      if (resp.project.ontology) {
        setOntology(resp.project.ontology as OntologyIR);
      }
      setReport(resp.reconcile_report);
      setPending(null);
      toast.success("Reconcile decisions applied");
    } catch (err) {
      toast.error("Failed to apply reconcile decisions", {
        description: err instanceof Error ? err.message : "Unknown error",
      });
    } finally {
      setApplying(false);
    }
  };

  return (
    <div className="absolute left-1/2 top-3 z-10 -translate-x-1/2">
      <div
        className={cn(
          "rounded-lg border shadow-lg backdrop-blur-sm",
          report.confidence === "low"
            ? "border-red-200 bg-red-50/95 dark:border-red-900 dark:bg-red-950/95"
            : report.confidence === "medium"
              ? "border-amber-200 bg-amber-50/95 dark:border-amber-900 dark:bg-amber-950/95"
              : "border-emerald-200 bg-emerald-50/95 dark:border-emerald-900 dark:bg-emerald-950/95",
        )}
      >
        {/* Summary row */}
        <div className="flex items-center gap-3 px-4 py-2 text-xs">
          <span className="font-semibold text-zinc-700 dark:text-zinc-300">
            Refine diff
          </span>
          <ConfidenceBadge confidence={report.confidence} />
          {addedCount > 0 && (
            <span className="text-emerald-600 dark:text-emerald-400">
              +{addedCount} new
            </span>
          )}
          {uncertainCount > 0 && (
            <button
              onClick={() => pending && setExpanded((v) => !v)}
              className={cn(
                "text-amber-600 dark:text-amber-400",
                pending && "cursor-pointer underline decoration-dotted",
              )}
            >
              ~{uncertainCount} uncertain
            </button>
          )}
          {deletedCount > 0 && (
            <span className="text-red-600 dark:text-red-400">
              -{deletedCount} removed
            </span>
          )}
          <span className="text-zinc-400">
            {report.preserved_ids.length} preserved
          </span>
          {pending && (
            <button
              onClick={handleApplyDecisions}
              disabled={applying}
              className={cn(
                "ml-1 rounded-md bg-emerald-600 px-3 py-1 text-white hover:bg-emerald-700 disabled:opacity-50",
                applying && "cursor-wait",
              )}
            >
              {applying ? "Applying..." : "Apply Decisions"}
            </button>
          )}
          <button
            onClick={handleDismiss}
            className="ml-1 rounded-md px-2 py-0.5 text-zinc-500 hover:bg-white/50 hover:text-zinc-700 dark:hover:bg-zinc-800/50"
          >
            Dismiss
          </button>
        </div>

        {/* Expanded uncertain match list */}
        {pending && expanded && uncertainCount > 0 && (
          <div className="border-t border-zinc-200 px-4 py-2 dark:border-zinc-700">
            <div className="max-h-48 space-y-1.5 overflow-y-auto">
              {pending.report.uncertain_matches.map((m) => (
                <div
                  key={m.original_id}
                  className="flex items-center gap-2 rounded px-2 py-1 text-xs hover:bg-white/50 dark:hover:bg-zinc-800/30"
                >
                  <span className="min-w-0 flex-1 truncate text-zinc-700 dark:text-zinc-300">
                    <span className="font-medium">{m.original_label}</span>
                    {m.original_label !== m.matched_label && (
                      <span className="text-zinc-400">
                        {" -> "}
                        {m.matched_label}
                      </span>
                    )}
                    <span className="ml-1 text-zinc-400">
                      ({m.entity_kind})
                    </span>
                    <span className="ml-1 italic text-zinc-400">
                      {m.match_reason}
                    </span>
                  </span>
                  <button
                    onClick={() => toggleDecision(m.original_id)}
                    className={cn(
                      "shrink-0 rounded px-2 py-0.5 text-[10px] font-semibold uppercase transition-colors",
                      decisions[m.original_id]
                        ? "bg-emerald-100 text-emerald-700 hover:bg-emerald-200 dark:bg-emerald-900 dark:text-emerald-300 dark:hover:bg-emerald-800"
                        : "bg-red-100 text-red-700 hover:bg-red-200 dark:bg-red-900 dark:text-red-300 dark:hover:bg-red-800",
                    )}
                  >
                    {decisions[m.original_id] ? "Accept" : "Reject"}
                  </button>
                </div>
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

function ConfidenceBadge({
  confidence,
}: {
  confidence: ReconcileReport["confidence"];
}) {
  return (
    <span
      className={cn(
        "rounded px-1.5 py-0.5 text-[9px] font-bold uppercase",
        confidence === "high"
          ? "bg-emerald-100 text-emerald-700 dark:bg-emerald-900 dark:text-emerald-300"
          : confidence === "medium"
            ? "bg-amber-100 text-amber-700 dark:bg-amber-900 dark:text-amber-300"
            : "bg-red-100 text-red-700 dark:bg-red-900 dark:text-red-300",
      )}
    >
      {confidence}
    </span>
  );
}
