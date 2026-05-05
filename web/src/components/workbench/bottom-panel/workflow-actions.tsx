"use client";

import { useState } from "react";
import { useTranslations } from "next-intl";
import { Check, Trash2, Wand2 } from "lucide-react";
import { toast } from "@/components/ui/toast";
import { Heading } from "@/components/ui/heading";

import { Spinner } from "@/components/ui/spinner";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { FormInput } from "@/components/ui/form-input";
import { useConfirm } from "@/components/providers/confirm-provider";
import {
  DesignGateChecklist,
  focusFirstUnmetGate,
} from "@/components/workbench/design/design-gate-checklist";
import { cn } from "@/lib/cn";
import {
  ApiError,
  completeOntologyDraft,
  deleteOntologyDraft,
  deploySchema,
  designOntologyDraftStream,
  updateDecisions,
} from "@/lib/api";
import { useGuardPendingEdits } from "@/lib/guard-pending-edits";
import { arr } from "@/lib/ir-collections";
import type { DesignOptions, OntologyDraft } from "@/types/api";

import {
  WorkflowStatusBadge,
  columnKey,
  relationshipKey,
} from "./design-panel-shared";
import { ProgressIndicator, SourceHistorySection } from "./workflow-indicators";
import { useWorkflowFormState } from "./use-workflow-form-state";
import type { DesignDecisions } from "./use-design-decisions";
import { DeploymentActions } from "./deployment-actions";
import { EnhanceActions } from "./enhance-actions";
import { GraphAuditSection } from "./graph-audit-section";

export interface WorkflowActionsProps extends DesignDecisions {
  project: OntologyDraft;
  loading: boolean;
  setLoading: (v: boolean) => void;
  /**
   * Atomic project + ontology cache update — see
   * `OntologySlice.applyProjectSnapshot`. Every analyse / design /
   * complete / extend / refine action lands its server response
   * through this single entry point.
   */
  applyProjectSnapshot: (project: OntologyDraft | null) => void;
  onApiError: (err: unknown, label: string) => Promise<boolean>;
  /** Ref to the analysis review <details> element in the right panel */
  analysisRef: React.RefObject<HTMLDetailsElement | null>;
}

export function WorkflowActions({
  project,
  loading,
  setLoading,
  applyProjectSnapshot,
  onApiError,
  analysisRef,
  confirmedRelationships,
  piiAnnotations,
  excludedColumns,
  clarifications,
  excludedTables,
  partialAnalysisAcknowledged,
  largeSchemaAcknowledged,
}: WorkflowActionsProps) {
  const t = useTranslations("workbench.bottomPanel.workflowActions");
  const tCommon = useTranslations("common");
  const tErrors = useTranslations("errors");
  const report = project.analysis_report;
  const [progressPhase, setProgressPhase] = useState<string | null>(null);
  const [progressDetail, setProgressDetail] = useState<string | null>(null);
  const guardPendingEdits = useGuardPendingEdits();
  const confirmDialog = useConfirm();

  const form = useWorkflowFormState(
    project.id,
    project.title,
    project.source_config.schema_name,
  );
  const isCompleted = project.status === "completed";
  const isDesigned = project.status === "designed";

  // Server is the single source of truth for design eligibility.
  // `design_gates` is computed by `evaluate_design_gates` on every
  // project response; we only check whether any blocking gate is
  // unmet here. The DesignGateChecklist component renders the
  // detailed status alongside the disabled button.
  const designGates = project.design_gates ?? [];
  const blockingUnmetCount = designGates.filter(
    (g) => g.blocks_design && g.status === "unmet",
  ).length;
  const canDesign = !loading && !isCompleted && blockingUnmetCount === 0;

  function buildDesignOptions(): DesignOptions {
    if (!report) return {};
    return {
      confirmed_relationships: report.implied_relationships
        .filter((rel) => confirmedRelationships[relationshipKey(rel)])
        .map((rel) => ({
          from_table: rel.from_table,
          from_column: rel.from_column,
          to_table: rel.to_table,
          to_column: rel.to_column,
        })),
      pii_annotations: Object.values(piiAnnotations).map((entry) => ({
        table: entry.table,
        column: entry.column,
        kind: entry.kind,
      })),
      excluded_columns: Object.values(excludedColumns).map((entry) => ({
        table: entry.table,
        column: entry.column,
      })),
      excluded_tables: report.table_exclusion_suggestions
        .filter((s) => excludedTables[s.table_name])
        .map((s) => s.table_name),
      column_clarifications: report.ambiguous_columns
        .map((col) => {
          const hint = clarifications[columnKey(col.column.relation, col.column.column)]?.trim();
          if (!hint) return null;
          return { table: col.column.relation, column: col.column.column, hint };
        })
        .filter((e): e is NonNullable<typeof e> => e !== null),
      partial_analysis_acknowledged: partialAnalysisAcknowledged,
      large_schema_acknowledged: largeSchemaAcknowledged,
    };
  }

  async function handleSaveDecisions() {
    setLoading(true);
    try {
      const updated = await updateDecisions(project.id, {
        design_options: buildDesignOptions(),
        revision: project.revision,
      });
      applyProjectSnapshot(updated);
      toast.success(t("decisionsSaved"));
    } catch (err) {
      if (await onApiError(err, t("decisionsSaveFailed"))) return;
    } finally {
      setLoading(false);
    }
  }

  async function handleDesign() {
    if (!(await guardPendingEdits(t("guardActions.design")))) return;
    setLoading(true);
    setProgressPhase(null);
    setProgressDetail(null);
    try {
      const saved = await updateDecisions(project.id, {
        design_options: buildDesignOptions(),
        revision: project.revision,
      });

      let streamErrorType = "";
      let streamErrorMsg = "";

      await designOntologyDraftStream(saved.id, {
        revision: saved.revision,
        context: form.design.designContext.trim() || undefined,
      }, {
        onPhase: (phase, detail) => {
          setProgressPhase(phase);
          setProgressDetail(detail ?? null);
        },
        onResult: (resp) => {
          applyProjectSnapshot(resp.project);
          toast.success(t("ontologyDesigned"), {
            description: resp.project.ontology
              ? t("completeDesignedDescription", {
                  nodeCount: arr(resp.project.ontology.node_types).length,
                  edgeCount: arr(resp.project.ontology.edge_types).length,
                })
              : undefined,
          });
        },
        onError: (errorType, message) => {
          streamErrorType = errorType;
          streamErrorMsg = message;
        },
      });

      if (streamErrorMsg) {
        // SSE error events ship `{ code, class, params: { detail } }`.
        // The catalog `errors.<code>` template interpolates `detail`.
        toast.error(t("designFailed"), {
          description: streamErrorType
            ? tErrors(streamErrorType, { detail: streamErrorMsg })
            : streamErrorMsg,
        });
      }
    } catch (err) {
      if (await onApiError(err, t("designFailed"))) return;
    } finally {
      setLoading(false);
      setProgressPhase(null);
      setProgressDetail(null);
    }
  }

  async function handleComplete(acknowledgeRisks = false) {
    if (!(await guardPendingEdits(t("guardActions.complete")))) return;
    if (!form.complete.completeName.trim()) {
      toast.error(t("completeNameRequired"));
      return;
    }
    setLoading(true);
    try {
      const completed = await completeOntologyDraft(project.id, {
        revision: project.revision,
        name: form.complete.completeName.trim(),
        acknowledge_quality_risks: acknowledgeRisks || undefined,
      });
      applyProjectSnapshot(completed);
      if (form.complete.deployOnComplete) {
        try {
          await deploySchema(project.id, { dry_run: false });
          toast.success(t("completeWithDeploy"));
        } catch (deployErr) {
          const msg =
            deployErr instanceof ApiError ? deployErr.message : t("toast.unknownError");
          toast.warning(t("deployFailedPartial", { error: msg }));
        }
      } else {
        toast.success(t("completeSuccess"));
      }
    } catch (err) {
      if (err instanceof ApiError && err.code === "quality_gate") {
        const detail =
          (err.params.detail as string | undefined) ?? err.message;
        const ok = await confirmDialog({
          title: t("qualityGateWarningTitle"),
          description: t("qualityGateWarningDescription", { message: detail }),
          confirmLabel: t("qualityGateConfirmLabel"),
          variant: "warning",
        });
        if (ok) {
          return handleComplete(true);
        }
        setLoading(false);
        return;
      }
      if (await onApiError(err, t("completeFailed"))) return;
    } finally {
      setLoading(false);
    }
  }

  async function handleDelete() {
    // Phrase-match gate: user must type the project's title verbatim
    // to confirm. Foundry / GitHub / Stripe all gate destructive ops
    // on a phrase match — `delete` is too easy to autocomplete past
    // when distracted, and accidental project drops are unrecoverable.
    // Untitled projects fall back to a generic "delete" placeholder
    // because there's no resource label worth typing in that case.
    const phrase = project.title?.trim() || "delete";
    const ok = await confirmDialog({
      title: t("deleteProjectTitle"),
      description: t("deleteProjectDescription"),
      confirmLabel: t("deleteProjectConfirm"),
      variant: "danger",
      typeToConfirm: {
        phrase,
        label: t("deleteProjectTypeLabel"),
      },
    });
    if (!ok) return;
    setLoading(true);
    try {
      await deleteOntologyDraft(project.id);
      applyProjectSnapshot(null);
      toast.success(t("projectDeleted"));
    } catch (err) {
      if (await onApiError(err, t("deleteProjectFailed"))) return;
    } finally {
      setLoading(false);
    }
  }

  return (
    <>
      <div className="flex items-start justify-between gap-2">
        <div>
          <Heading level={3} size={6}>
            {project.title ?? t("untitledProject")}
          </Heading>
          <p className="mt-0.5 text-xs text-foreground-muted">
            {t("revMeta", {
              source: project.source_config.source_type,
              revision: project.revision,
            })}
          </p>
        </div>
        <div className="flex items-center gap-1.5">
          <WorkflowStatusBadge status={project.status} />
          <Button
            variant="ghost"
            size="sm"
            onClick={async () => {
              if (!(await guardPendingEdits(t("guardActions.closeProject")))) return;
              applyProjectSnapshot(null);
            }}
            className="text-xs"
          >
            {tCommon("close")}
          </Button>
          {!isCompleted && (
            <button type="button"
              onClick={handleDelete}
              disabled={loading}
              className="rounded p-1 text-foreground-muted hover:bg-danger-surface hover:text-danger-foreground"
            >
              <Trash2 className="h-3.5 w-3.5" />
            </button>
          )}
        </div>
      </div>

      {project.source_history?.length > 0 && (
        <SourceHistorySection entries={project.source_history} />
      )}

      {loading && (
        <ProgressIndicator
          phase={progressPhase ?? "starting"}
          detail={progressDetail}
        />
      )}

      {/* Analyzed: design is the primary action */}
      {!isDesigned && !isCompleted && (
        <>
          <div>
            <label className="mb-1 block text-xs font-medium text-foreground">
              {t("domainHintsLabel")}
            </label>
            <FormInput
              type="text"
              placeholder={t("domainHintsPlaceholder")}
              value={form.design.designContext}
              onChange={(e) => form.design.setDesignContext(e.target.value)}
            />
          </div>
          <DesignGateChecklist gates={designGates} />
          <Button
            size="sm"
            onClick={() => {
              if (canDesign) {
                handleDesign();
                return;
              }
              focusFirstUnmetGate(designGates);
            }}
            disabled={loading}
            data-design-allowed={canDesign}
            title={
              canDesign
                ? undefined
                : t("tipDisabledResolve", { count: blockingUnmetCount })
            }
            className={cn(
              "w-full text-xs",
              !canDesign &&
                "bg-surface-inset text-foreground-muted hover:bg-surface-inset",
            )}
          >
            {loading ? (
              <Spinner size="xs" className="me-1.5" />
            ) : (
              <Wand2 className="me-1.5 h-3.5 w-3.5" />
            )}
            {t("designOntology")}
          </Button>
          {report && (
            <Button
              variant="outline"
              size="sm"
              onClick={handleSaveDecisions}
              disabled={loading}
              className="w-full text-xs"
            >
              {t("saveDecisions")}
            </Button>
          )}
        </>
      )}

      {/* Designed/Completed: enhance + advanced */}
      {(isDesigned || isCompleted) && (
        <EnhanceActions
          project={project}
          loading={loading}
          setLoading={setLoading}
          applyProjectSnapshot={applyProjectSnapshot}
          onApiError={onApiError}
          onRedesign={handleDesign}
          analysisRef={analysisRef}
          extend={form.extend}
          reanalyze={form.reanalyze}
        />
      )}

      {/* Designed: bridge to completed */}
      {isDesigned && !isCompleted && (
        <div className="space-y-2 rounded-lg border border-brand-border bg-brand-surface p-3">
          <h4 className="text-xs font-semibold text-brand-foreground-strong">
            {t("finalize")}
          </h4>
          <FormInput
            type="text"
            placeholder={t("ontologyNamePlaceholder")}
            value={form.complete.completeName}
            onChange={(e) => form.complete.setCompleteName(e.target.value)}
          />
          <Checkbox
            checked={form.complete.deployOnComplete}
            onChange={(e) => form.complete.setDeployOnComplete(e.target.checked)}
            label={t("deployOnComplete")}
          />
          <Button
            size="sm"
            onClick={() => handleComplete()}
            disabled={loading || !form.complete.completeName.trim()}
            className="w-full text-xs"
          >
            {loading ? (
              <Spinner size="xs" className="me-1.5" />
            ) : (
              <Check className="me-1.5 h-3 w-3" />
            )}
            {t("complete")}
          </Button>
        </div>
      )}

      {/* Completed status */}
      {isCompleted && (
        <div className="space-y-2 rounded-lg border border-brand-border bg-brand-surface p-3">
          <div className="flex items-center gap-2">
            <Check className="h-4 w-4 text-brand-foreground" />
            <h4 className="text-xs font-semibold text-brand-foreground-strong">
              {t("savedHeader")}
            </h4>
          </div>
          <p className="text-xs text-brand-foreground">
            {project.ontology_id ? t("savedDescription") : t("completedDescription")}
          </p>
        </div>
      )}

      {/* Completed: deployment + load + graph audit */}
      {isCompleted && project.ontology && (
        <DeploymentActions
          projectId={project.id}
          loading={loading}
          setLoading={setLoading}
          onApiError={onApiError}
          deployPreview={form.deploy.deployPreview}
          setDeployPreview={form.deploy.setDeployPreview}
          loadPlan={form.deploy.loadPlan}
          setLoadPlan={form.deploy.setLoadPlan}
        />
      )}

      {isCompleted && <GraphAuditSection />}
    </>
  );
}
