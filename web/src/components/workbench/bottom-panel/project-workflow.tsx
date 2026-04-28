"use client";

import { useRef, useState } from "react";
import { useTranslations } from "next-intl";
import { ApiError, getProject } from "@/lib/api";
import { cn } from "@/lib/cn";
import { toast } from "sonner";
import type { DesignProject} from "@/types/api";import { AnalysisReviewSection } from "./analysis-review-section";
import { useAppStore } from "@/lib/store";
import { WorkflowActions } from "./workflow-actions";
import { PhaseStepper } from "./phase-stepper";
import { RevisionHistoryPanel } from "./revision-history-panel";
import { useDesignDecisions } from "./use-design-decisions";

// ---------------------------------------------------------------------------
// Project Workflow — orchestrator
// ---------------------------------------------------------------------------

const STATUS_STEPS = ["analyzed", "designed", "completed"] as const;

export function ProjectWorkflow({
  project,
  applyProjectSnapshot,
}: {
  project: DesignProject;
  /**
   * Atomic project + ontology cache update — see
   * `OntologySlice.applyProjectSnapshot`. Workflow actions land
   * their server response through this single entry point so
   * `activeProject` and the ontology cache cannot drift.
   */
  applyProjectSnapshot: (project: DesignProject | null) => void;
}) {
  const t = useTranslations("workbench.bottomPanel.workflow");
  const tActions = useTranslations("workbench.bottomPanel.workflowActions");
  const report = project.analysis_report;
  const [loading, setLoading] = useState(false);
  const analysisRef = useRef<HTMLDetailsElement>(null);

  const decisions = useDesignDecisions(project.design_options, report);

  // Shared error handler
  async function handleApiError(err: unknown, label: string): Promise<boolean> {
    if (err instanceof ApiError && err.type === "conflict") {
      toast.error(tActions("conflictTitle"), {
        description: tActions("conflictDescription"),
      });
      try {
        const fresh = await getProject(project.id);
        applyProjectSnapshot(fresh);
      } catch {
        /* ignore reload failure */
      }
      return true;
    }
    // Structured 422 from `enforce_design_gates`: backend ships the
    // exact unmet gate ids — surface them inline instead of dumping
    // the raw JSON into a toast. The freshly-fetched project carries
    // the same `design_gates` vector the FE renders, so the
    // checklist updates the moment we reload.
    if (err instanceof ApiError && err.type === "design_gates_unmet") {
      const unmet = extractUnmetIds(err.details);
      toast.error(tActions("toast.designGatesUnmetTitle"), {
        description: tActions("toast.designGatesUnmetDescription", {
          count: unmet.length,
        }),
      });
      try {
        const fresh = await getProject(project.id);
        applyProjectSnapshot(fresh);
      } catch {
        /* ignore reload failure */
      }
      return true;
    }
    toast.error(label, {
      description: err instanceof Error ? err.message : tActions("toast.unknownError"),
    });
    return false;
  }

  /** Pull `unmet: string[]` out of the backend's structured 422
   *  `details` payload. Defensive against a missing field — older
   *  servers in a rolling deploy may surface a different shape. */
  function extractUnmetIds(details: unknown): string[] {
    if (!details || typeof details !== "object") return [];
    const unmet = (details as { unmet?: unknown }).unmet;
    if (!Array.isArray(unmet)) return [];
    return unmet.filter((v): v is string => typeof v === "string");
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
        <PhaseStepper currentStepIndex={currentStepIndex} />

        {/* Stale-report advisory: backend signalled the persisted
            analysis_report can't deserialize against the current
            schema. Design proceeds (gates skipped) but operator
            should re-run analyse so gate enforcement comes back. */}
        {project.analysis_report_status === "stale" && (
          <div className="rounded-md border border-amber-300 bg-amber-50 px-3 py-2 dark:border-amber-700 dark:bg-amber-950/40">
            <p className="text-xs font-medium text-amber-900 dark:text-amber-100">
              {t("staleReportTitle")}
            </p>
            <p className="mt-0.5 text-[11px] text-amber-800 dark:text-amber-200">
              {t("staleReportHint")}
            </p>
          </div>
        )}

        {/* Contextual status guide */}
        {project.status === "analyzed" && (
          <p className="px-2 text-xs text-muted-foreground">
            {t("analyzedGuidance")}
          </p>
        )}
        {project.status === "designed" && (
          <p className="px-2 text-xs text-muted-foreground">
            {t.rich("designedGuidance", {
              kbd: (chunks) => (
                <kbd className="rounded bg-zinc-200 px-1 py-0.5 font-mono text-[9px] dark:bg-zinc-700">
                  {chunks}
                </kbd>
              ),
            })}
          </p>
        )}
        {project.status === "completed" && (
          <p className="px-2 text-xs text-muted-foreground">
            {t("completedGuidance")}
          </p>
        )}

        {/* Delegated actions panel */}
        <WorkflowActions
          project={project}
          loading={loading}
          setLoading={setLoading}
          applyProjectSnapshot={applyProjectSnapshot}
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
              <h4 className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                {t("qualityHeader")}
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
                {project.quality_report.confidence === "high"
                  ? t("confidenceHigh")
                  : project.quality_report.confidence === "medium"
                    ? t("confidenceMedium")
                    : t("confidenceLow")}
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
                    {high > 0 && <span className="rounded-full bg-red-100 px-1.5 py-0.5 text-xs font-medium text-red-700 dark:bg-red-950/60 dark:text-red-300">{t("highSeverity", { count: high })}</span>}
                    {medium > 0 && <span className="rounded-full bg-amber-100 px-1.5 py-0.5 text-xs font-medium text-amber-700 dark:bg-amber-950/60 dark:text-amber-300">{t("mediumSeverity", { count: medium })}</span>}
                    {low > 0 && <span className="rounded-full bg-zinc-100 px-1.5 py-0.5 text-xs font-medium text-zinc-600 dark:bg-zinc-800 dark:text-muted-foreground">{t("lowSeverity", { count: low })}</span>}
                  </>
                );
              })()}
            </div>
            {/* Guidance */}
            <p className="mt-2 text-xs text-muted-foreground">
              {project.quality_report.confidence === "high" && t("guidanceHigh")}
              {project.quality_report.confidence === "medium" && t("guidanceMedium")}
              {project.quality_report.confidence === "low" && t("guidanceLow")}
            </p>
            {/* Link to Quality tab */}
            <button
              onClick={() => useAppStore.getState().setDesignBottomTab("quality")}
              className="mt-1.5 text-xs font-medium text-emerald-600 hover:text-emerald-700 dark:text-emerald-400"
            >
              {t("viewFullReport")}
            </button>
          </div>
        )}

        {/* Revision history */}
        {(isDesigned || isCompleted) && (
          <RevisionHistoryPanel
            project={project}
            loading={loading}
            setLoading={setLoading}
            applyProjectSnapshot={applyProjectSnapshot}
            onApiError={handleApiError}
          />
        )}

        {/* Analysis review */}
        {report && !isCompleted && (
          <details ref={analysisRef} open={!isDesigned}>
            <summary className="cursor-pointer text-xs font-semibold uppercase tracking-wider text-muted-foreground hover:text-zinc-700 dark:hover:text-zinc-300">
              {t("analysisReview")}
              <span className="ml-2 text-xs font-normal normal-case text-muted-foreground">
                {decisions.unresolvedClarificationCount > 0
                  ? t("unresolved", {
                      count: decisions.unresolvedClarificationCount,
                    })
                  : t("allResolved")}
              </span>
            </summary>
            <div className="mt-2">
              <AnalysisReviewSection
                report={report}
                confirmedRelationships={decisions.confirmedRelationships}
                setConfirmedRelationships={decisions.setConfirmedRelationships}
                piiAnnotations={decisions.piiAnnotations}
                setPiiAnnotations={decisions.setPiiAnnotations}
                excludedColumns={decisions.excludedColumns}
                setExcludedColumns={decisions.setExcludedColumns}
                clarifications={decisions.clarifications}
                setClarifications={decisions.setClarifications}
                excludedTables={decisions.excludedTables}
                setExcludedTables={decisions.setExcludedTables}
                partialAnalysisAcknowledged={decisions.partialAnalysisAcknowledged}
                setPartialAnalysisAcknowledged={decisions.setPartialAnalysisAcknowledged}
                largeSchemaAcknowledged={decisions.largeSchemaAcknowledged}
                setLargeSchemaAcknowledged={decisions.setLargeSchemaAcknowledged}
                unresolvedClarificationCount={decisions.unresolvedClarificationCount}
              />
            </div>
          </details>
        )}
      </div>
    </div>
  );
}
