"use client";

import { useRef, useState } from "react";
import { ApiError, getProject } from "@/lib/api";
import { cn } from "@/lib/cn";
import { toast } from "sonner";
import type { DesignProject, OntologyIR } from "@/types/api";
import { AnalysisReviewSection } from "./analysis-review-section";
import { useAppStore } from "@/lib/store";
import { WorkflowActions } from "./workflow-actions";
import { RevisionHistoryPanel } from "./revision-history-panel";
import { useDesignDecisions } from "./use-design-decisions";

// ---------------------------------------------------------------------------
// Project Workflow — orchestrator
// ---------------------------------------------------------------------------

const STATUS_STEPS = ["analyzed", "designed", "completed"] as const;

export function ProjectWorkflow({
  project,
  setProject,
  setOntology,
}: {
  project: DesignProject;
  setProject: (p: DesignProject | null) => void;
  setOntology: (o: OntologyIR) => void;
}) {
  const report = project.analysis_report;
  const [loading, setLoading] = useState(false);
  const analysisRef = useRef<HTMLDetailsElement>(null);

  const decisions = useDesignDecisions(project.design_options, report);

  // Shared error handler
  async function handleApiError(err: unknown, label: string): Promise<boolean> {
    if (err instanceof ApiError && err.type === "conflict") {
      toast.error("Conflict: project was modified elsewhere", {
        description: "Reloading latest version.",
      });
      try {
        const fresh = await getProject(project.id);
        setProject(fresh);
      } catch {
        /* ignore reload failure */
      }
      return true;
    }
    toast.error(label, {
      description: err instanceof Error ? err.message : "Unknown error",
    });
    return false;
  }

  // Step indicator
  const currentStepIndex = STATUS_STEPS.indexOf(
    project.status as (typeof STATUS_STEPS)[number],
  );
  const isDesigned = project.status === "designed";
  const isCompleted = project.status === "completed";

  return (
    <div className="flex gap-6 p-4">
      {/* Left: project info + actions — responsive width */}
      <div className="w-80 shrink-0 space-y-3 xl:w-96 2xl:w-[480px]">
        {/* Step indicator */}
        <div className="flex items-center justify-between px-2">
          {STATUS_STEPS.map((step, i) => (
            <div key={step} className="flex items-center">
              <div className="flex flex-col items-center gap-1">
                <div
                  className={cn(
                    "flex h-5 w-5 items-center justify-center rounded-full text-[9px] font-bold",
                    i <= currentStepIndex
                      ? "bg-emerald-500 text-white"
                      : "bg-zinc-200 text-zinc-400 dark:bg-zinc-700 dark:text-zinc-500",
                  )}
                >
                  {i + 1}
                </div>
                <span
                  className={cn(
                    "text-[9px] font-medium capitalize",
                    i <= currentStepIndex
                      ? "text-emerald-600 dark:text-emerald-400"
                      : "text-zinc-400 dark:text-zinc-500",
                  )}
                >
                  {step === "analyzed" ? "Analyze" : step === "designed" ? "Design" : "Complete"}
                </span>
              </div>
              {i < STATUS_STEPS.length - 1 && (
                <div
                  className={cn(
                    "mx-2 h-px w-8",
                    i < currentStepIndex
                      ? "bg-emerald-400"
                      : "bg-zinc-200 dark:bg-zinc-700",
                  )}
                />
              )}
            </div>
          ))}
        </div>

        {/* Contextual status guide */}
        {project.status === "analyzed" && (
          <p className="px-2 text-xs text-zinc-500">
            Review the analysis results and resolve PII/Clarification decisions, then click Design Ontology.
          </p>
        )}
        {project.status === "designed" && (
          <p className="px-2 text-xs text-zinc-500">
            Review the ontology on the canvas. Use ⌘K to edit, then Complete &amp; Save when ready.
          </p>
        )}
        {project.status === "completed" && (
          <p className="px-2 text-xs text-zinc-500">
            Ontology saved. Use Analyze mode to query, or Fork to create a new version.
          </p>
        )}

        {/* Delegated actions panel */}
        <WorkflowActions
          project={project}
          loading={loading}
          setLoading={setLoading}
          setProject={setProject}
          setOntology={setOntology}
          onApiError={handleApiError}
          analysisRef={analysisRef}
          {...decisions}
        />
      </div>

      {/* Right: quality report + schema warning + revision history + analysis review */}
      <div className="flex-1 space-y-3 overflow-auto">
        {/* Large schema info is shown in the workflow-actions checkbox — no duplicate here */}

        {/* Quality summary (detail in Quality tab) */}
        {project.quality_report && (
          <div className="rounded-lg border border-zinc-200 bg-zinc-50/70 p-3 dark:border-zinc-800 dark:bg-zinc-900/50">
            <div className="flex items-center justify-between">
              <h4 className="text-xs font-semibold uppercase tracking-wider text-zinc-500">
                Quality
              </h4>
              <span
                className={cn(
                  "rounded-full px-1.5 py-0.5 text-[9px] font-medium uppercase",
                  project.quality_report.confidence === "high"
                    ? "bg-emerald-100 text-emerald-800 dark:bg-emerald-950/60 dark:text-emerald-200"
                    : project.quality_report.confidence === "medium"
                      ? "bg-amber-100 text-amber-800 dark:bg-amber-950/60 dark:text-amber-200"
                      : "bg-red-100 text-red-800 dark:bg-red-950/60 dark:text-red-200",
                )}
              >
                {project.quality_report.confidence}
              </span>
            </div>
            {/* Counts summary */}
            <div className="mt-2 flex items-center gap-2">
              {(() => {
                const gaps = project.quality_report.gaps;
                const high = gaps.filter((g) => g.severity === "high").length;
                const medium = gaps.filter((g) => g.severity === "medium").length;
                const low = gaps.filter((g) => g.severity === "low").length;
                return (
                  <>
                    {high > 0 && <span className="rounded-full bg-red-100 px-1.5 py-0.5 text-[10px] font-medium text-red-700 dark:bg-red-950/60 dark:text-red-300">{high} High</span>}
                    {medium > 0 && <span className="rounded-full bg-amber-100 px-1.5 py-0.5 text-[10px] font-medium text-amber-700 dark:bg-amber-950/60 dark:text-amber-300">{medium} Medium</span>}
                    {low > 0 && <span className="rounded-full bg-zinc-100 px-1.5 py-0.5 text-[10px] font-medium text-zinc-600 dark:bg-zinc-800 dark:text-zinc-400">{low} Low</span>}
                  </>
                );
              })()}
            </div>
            {/* Guidance */}
            <p className="mt-2 text-[10px] text-zinc-500">
              {project.quality_report.confidence === "high" && "\u2713 High confidence — ready to finalize."}
              {project.quality_report.confidence === "medium" && "Consider refining with \u2318K before completing."}
              {project.quality_report.confidence === "low" && "\u26a0 Low confidence — review quality gaps before proceeding."}
            </p>
            {/* Link to Quality tab */}
            <button
              onClick={() => useAppStore.getState().setDesignBottomTab("quality")}
              className="mt-1.5 text-[10px] font-medium text-emerald-600 hover:text-emerald-700 dark:text-emerald-400"
            >
              View full report →
            </button>
          </div>
        )}

        {/* Revision history */}
        {(isDesigned || isCompleted) && (
          <RevisionHistoryPanel
            project={project}
            loading={loading}
            setLoading={setLoading}
            setProject={setProject}
            setOntology={setOntology}
            onApiError={handleApiError}
          />
        )}

        {/* Analysis review */}
        {report && !isCompleted && (
          <details ref={analysisRef} open={!isDesigned}>
            <summary className="cursor-pointer text-xs font-semibold uppercase tracking-wider text-zinc-500 hover:text-zinc-700 dark:hover:text-zinc-300">
              Analysis Review
              <span className="ml-2 text-[10px] font-normal normal-case text-zinc-400">
                {decisions.unresolvedPiiCount + decisions.unresolvedClarificationCount > 0
                  ? `${decisions.unresolvedPiiCount + decisions.unresolvedClarificationCount} unresolved`
                  : "all resolved"}
              </span>
            </summary>
            <div className="mt-2">
              <AnalysisReviewSection
                report={report}
                confirmedRelationships={decisions.confirmedRelationships}
                setConfirmedRelationships={decisions.setConfirmedRelationships}
                piiDecisions={decisions.piiDecisions}
                setPiiDecisions={decisions.setPiiDecisions}
                clarifications={decisions.clarifications}
                setClarifications={decisions.setClarifications}
                excludedTables={decisions.excludedTables}
                setExcludedTables={decisions.setExcludedTables}
                allowPartialAnalysis={decisions.allowPartialAnalysis}
                setAllowPartialAnalysis={decisions.setAllowPartialAnalysis}
                unresolvedPiiCount={decisions.unresolvedPiiCount}
                unresolvedClarificationCount={decisions.unresolvedClarificationCount}
                needsPartialAcknowledgement={decisions.needsPartialAcknowledgement}
              />
            </div>
          </details>
        )}
      </div>
    </div>
  );
}
