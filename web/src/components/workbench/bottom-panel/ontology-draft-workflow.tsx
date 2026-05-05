"use client";

import { useRef, useState } from "react";
import { useTranslations } from "next-intl";
import { ApiError, getOntologyDraft } from "@/lib/api";
import { cn } from "@/lib/cn";
import { toast } from "@/components/ui/toast";
import { KeyboardShortcut } from "@/components/ui/keyboard-shortcut";
import type { OntologyDraft} from "@/types/api";import { AnalysisReviewSection } from "./analysis-review-section";
import { useAppStore } from "@/lib/store";
import { WorkflowActions } from "./workflow-actions";
import { PhaseStepper } from "./phase-stepper";
import { RevisionHistoryPanel } from "./revision-history-panel";
import { useDesignDecisions } from "./use-design-decisions";

// ---------------------------------------------------------------------------
// Ontology Draft Workflow — orchestrator
// ---------------------------------------------------------------------------

const STATUS_STEPS = ["analyzed", "designed", "completed"] as const;

export function OntologyDraftWorkflow({
  project,
  applyOntologyDraftSnapshot,
}: {
  project: OntologyDraft;
  /**
   * Atomic project + ontology cache update — see
   * `OntologySlice.applyOntologyDraftSnapshot`. Workflow actions land
   * their server response through this single entry point so
   * `activeOntologyDraft` and the ontology cache cannot drift.
   */
  applyOntologyDraftSnapshot: (project: OntologyDraft | null) => void;
}) {
  const t = useTranslations("workbench.bottomPanel.workflow");
  const tActions = useTranslations("workbench.bottomPanel.workflowActions");
  const report = project.analysis_report;
  const [loading, setLoading] = useState(false);
  const analysisRef = useRef<HTMLDetailsElement>(null);

  const decisions = useDesignDecisions(project.design_options, report);

  // Shared error handler
  async function handleApiError(err: unknown, label: string): Promise<boolean> {
    if (err instanceof ApiError && err.code === "conflict") {
      toast.error(tActions("conflictTitle"), {
        description: tActions("conflictDescription"),
      });
      try {
        const fresh = await getOntologyDraft(project.id);
        applyOntologyDraftSnapshot(fresh);
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
    if (err instanceof ApiError && err.code === "design_gates_unmet") {
      const unmet = extractUnmetIds(err.params.details);
      toast.error(tActions("toast.designGatesUnmetTitle"), {
        description: tActions("toast.designGatesUnmetDescription", {
          count: unmet.length,
        }),
      });
      try {
        const fresh = await getOntologyDraft(project.id);
        applyOntologyDraftSnapshot(fresh);
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

  // Quality is summarised here as a compact one-line pill row; the
  // full per-gap detail lives in the Quality tab. Workflow used to
  // render a left/right split with a duplicate quality card on the
  // right — that card carried no information the Quality tab didn't
  // already own, just split the user's attention.
  const quality = project.quality_report;
  const gapCounts = quality
    ? {
        high: quality.gaps.filter((g) => g.severity === "high").length,
        medium: quality.gaps.filter((g) => g.severity === "medium").length,
        low: quality.gaps.filter((g) => g.severity === "low").length,
      }
    : null;

  return (
    <div className="mx-auto w-full max-w-[64rem] space-y-3 p-4">
      <PhaseStepper currentStepIndex={currentStepIndex} />

      {/* Stale-report advisory */}
      {project.analysis_report_status === "stale" && (
        <div className="rounded-md border border-warning-border bg-warning-surface px-3 py-2">
          <p className="text-xs font-medium text-warning-foreground">
            {t("staleReportTitle")}
          </p>
          <p className="mt-0.5 text-2xs text-warning-foreground">
            {t("staleReportHint")}
          </p>
        </div>
      )}

      {/* Quality summary — single line: confidence pill + gap counts +
          link to Quality tab. Replaces the old card-chrome duplicate. */}
      {quality && gapCounts && (
        <div className="flex flex-wrap items-center gap-2 rounded-md border border-divider bg-surface-raised px-3 py-2 text-xs">
          <span className="font-semibold uppercase tracking-wider text-foreground-muted">
            {t("qualityHeader")}
          </span>
          <span
            className={cn(
              "rounded-full px-1.5 py-0.5 text-2xs font-medium uppercase",
              quality.confidence === "high"
                ? "bg-brand-surface-strong text-brand-foreground-strong"
                : quality.confidence === "medium"
                  ? "bg-warning-surface text-warning-foreground"
                  : "bg-danger-surface text-danger-foreground",
            )}
          >
            {quality.confidence === "high"
              ? t("confidenceHigh")
              : quality.confidence === "medium"
                ? t("confidenceMedium")
                : t("confidenceLow")}
          </span>
          {gapCounts.high > 0 && (
            <span className="rounded-full bg-danger-surface px-1.5 py-0.5 text-2xs font-medium text-danger-foreground">
              {t("highSeverity", { count: gapCounts.high })}
            </span>
          )}
          {gapCounts.medium > 0 && (
            <span className="rounded-full bg-warning-surface px-1.5 py-0.5 text-2xs font-medium text-warning-foreground">
              {t("mediumSeverity", { count: gapCounts.medium })}
            </span>
          )}
          {gapCounts.low > 0 && (
            <span className="rounded-full bg-surface-inset px-1.5 py-0.5 text-2xs font-medium text-foreground">
              {t("lowSeverity", { count: gapCounts.low })}
            </span>
          )}
          <button
            type="button"
            onClick={() => useAppStore.getState().setDesignBottomTab("quality")}
            className="ms-auto text-xs font-medium text-brand-foreground hover:text-brand-foreground-strong"
          >
            {t("viewFullReport")}
          </button>
        </div>
      )}

      {/* Contextual status guide */}
      {project.status === "analyzed" && (
        <p className="px-2 text-xs text-foreground-muted">
          {t("analyzedGuidance")}
        </p>
      )}
      {project.status === "designed" && (
        <p className="px-2 text-xs text-foreground-muted">
          {t.rich("designedGuidance", {
            kbd: () => <KeyboardShortcut keys="mod+k" />,
          })}
        </p>
      )}
      {project.status === "completed" && (
        <p className="px-2 text-xs text-foreground-muted">
          {t("completedGuidance")}
        </p>
      )}

      {/* Actions */}
      <WorkflowActions
        project={project}
        loading={loading}
        setLoading={setLoading}
        applyOntologyDraftSnapshot={applyOntologyDraftSnapshot}
        onApiError={handleApiError}
        analysisRef={analysisRef}
        {...decisions}
      />

      {/* Revision history */}
      {(isDesigned || isCompleted) && (
        <RevisionHistoryPanel
          project={project}
          loading={loading}
          setLoading={setLoading}
          applyOntologyDraftSnapshot={applyOntologyDraftSnapshot}
          onApiError={handleApiError}
        />
      )}

      {/* Analysis review */}
      {report && !isCompleted && (
        <details ref={analysisRef} open={!isDesigned}>
          <summary className="cursor-pointer text-xs font-semibold uppercase tracking-wider text-foreground-muted hover:text-foreground-muted">
            {t("analysisReview")}
            <span className="ms-2 text-xs font-normal normal-case text-foreground-muted">
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
  );
}
