"use client";

import { useState } from "react";
import { useTranslations } from "next-intl";
import { HugeiconsIcon } from "@hugeicons/react";
import {
  Tick01Icon,
  Delete01Icon,
  MagicWand01Icon,
} from "@hugeicons/core-free-icons";
import { toast } from "sonner";

import { Spinner } from "@/components/ui/spinner";
import { Button } from "@/components/ui/button";
import { FormInput } from "@/components/ui/form-input";
import { useConfirm } from "@/components/ui/confirm-dialog";
import { useAppStore } from "@/lib/store";
import {
  ApiError,
  completeProject,
  deleteProject,
  deploySchema,
  designProjectStream,
  updateDecisions,
} from "@/lib/api";
import { useGuardPendingEdits } from "@/lib/guard-pending-edits";
import { errorMessage } from "@/lib/error-messages";
import { arr } from "@/lib/ir-collections";
import type {
  DesignOptions,
  DesignProject,
  OntologyIR,
} from "@/types/api";

import {
  StatusBadge,
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
  project: DesignProject;
  loading: boolean;
  setLoading: (v: boolean) => void;
  setProject: (p: DesignProject | null) => void;
  setOntology: (o: OntologyIR) => void;
  onApiError: (err: unknown, label: string) => Promise<boolean>;
  /** Ref to the analysis review <details> element in the right panel */
  analysisRef: React.RefObject<HTMLDetailsElement | null>;
}

export function WorkflowActions({
  project,
  loading,
  setLoading,
  setProject,
  setOntology,
  onApiError,
  analysisRef,
  confirmedRelationships,
  piiAnnotations,
  excludedColumns,
  clarifications,
  excludedTables,
  allowPartialAnalysis,
  unresolvedClarificationCount,
  needsPartialAcknowledgement,
}: WorkflowActionsProps) {
  const t = useTranslations("workbench.bottomPanel.workflowActions");
  const tCommon = useTranslations("common");
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
  const hasLargeSchema = (report?.schema_stats?.table_count ?? 0) > 100;

  const isCompleted = project.status === "completed";
  const isDesigned = project.status === "designed";

  const canDesign =
    !loading &&
    !isCompleted &&
    unresolvedClarificationCount === 0 &&
    !needsPartialAcknowledgement;

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
      allow_partial_source_analysis: allowPartialAnalysis,
    };
  }

  async function handleSaveDecisions() {
    setLoading(true);
    try {
      const updated = await updateDecisions(project.id, {
        design_options: buildDesignOptions(),
        revision: project.revision,
      });
      setProject(updated);
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

      await designProjectStream(saved.id, {
        revision: saved.revision,
        context: form.design.designContext.trim() || undefined,
        acknowledge_large_schema: hasLargeSchema ? true : undefined,
      }, {
        onPhase: (phase, detail) => {
          setProgressPhase(phase);
          setProgressDetail(detail ?? null);
        },
        onResult: (resp) => {
          setProject(resp.project);
          if (resp.project.ontology) {
            setOntology(resp.project.ontology);
          }
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
        toast.error(t("designFailed"), {
          description: errorMessage(streamErrorType, streamErrorMsg),
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
      const completed = await completeProject(project.id, {
        revision: project.revision,
        name: form.complete.completeName.trim(),
        acknowledge_quality_risks: acknowledgeRisks || undefined,
      });
      setProject(completed);
      if (completed.ontology) {
        setOntology(completed.ontology as OntologyIR);
      }
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
      if (err instanceof ApiError && err.type === "quality_gate") {
        const ok = await confirmDialog({
          title: t("qualityGateWarningTitle"),
          description: t("qualityGateWarningDescription", { message: err.message }),
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
    const ok = await confirmDialog({
      title: t("deleteProjectTitle"),
      description: t("deleteProjectDescription"),
      confirmLabel: t("deleteProjectConfirm"),
      variant: "danger",
    });
    if (!ok) return;
    setLoading(true);
    try {
      await deleteProject(project.id);
      setProject(null);
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
          <h3 className="text-sm font-semibold text-zinc-700 dark:text-zinc-300">
            {project.title ?? t("untitledProject")}
          </h3>
          <p className="mt-0.5 text-xs text-muted-foreground">
            {t("revMeta", {
              source: project.source_config.source_type,
              revision: project.revision,
            })}
          </p>
        </div>
        <div className="flex items-center gap-1.5">
          <StatusBadge status={project.status} />
          <Button
            variant="ghost"
            size="sm"
            onClick={async () => {
              if (!(await guardPendingEdits(t("guardActions.closeProject")))) return;
              setProject(null);
              useAppStore.getState().resetOntology();
            }}
            className="text-xs"
          >
            {tCommon("close")}
          </Button>
          {!isCompleted && (
            <button
              onClick={handleDelete}
              disabled={loading}
              className="rounded p-1 text-muted-foreground hover:bg-red-50 hover:text-red-500 dark:hover:bg-red-950"
            >
              <HugeiconsIcon icon={Delete01Icon} className="h-3.5 w-3.5" size="100%" />
            </button>
          )}
        </div>
      </div>

      {project.source_history?.length > 0 && (
        <SourceHistorySection entries={project.source_history} />
      )}

      {loading && progressPhase && (
        <ProgressIndicator phase={progressPhase} detail={progressDetail} />
      )}

      {/* Analyzed: design is the primary action */}
      {!isDesigned && !isCompleted && (
        <>
          <div>
            <label className="mb-1 block text-xs font-medium text-zinc-600 dark:text-muted-foreground">
              {t("domainHintsLabel")}
            </label>
            <FormInput
              type="text"
              placeholder={t("domainHintsPlaceholder")}
              value={form.design.designContext}
              onChange={(e) => form.design.setDesignContext(e.target.value)}
            />
          </div>
          {hasLargeSchema && (
            <label className="flex items-start gap-2 rounded-lg border border-amber-200 bg-amber-50 px-3 py-2 text-xs text-amber-700 dark:border-amber-800 dark:bg-amber-950/20 dark:text-amber-400">
              <input
                type="checkbox"
                checked={form.design.acknowledgeLargeSchema}
                onChange={(e) => form.design.setAcknowledgeLargeSchema(e.target.checked)}
                className="mt-0.5 h-3.5 w-3.5 shrink-0 rounded border-amber-300 text-amber-600"
              />
              <span>
                {t.rich("largeSchemaWarning", {
                  count: report?.schema_stats?.table_count ?? 0,
                  bold: (chunks) => <span className="font-medium">{chunks}</span>,
                })}
              </span>
            </label>
          )}
          <Button
            size="sm"
            onClick={handleDesign}
            disabled={!canDesign || (hasLargeSchema && !form.design.acknowledgeLargeSchema)}
            title={
              !canDesign && (unresolvedClarificationCount > 0 || needsPartialAcknowledgement)
                ? t("tipDisabledResolve")
                : hasLargeSchema && !form.design.acknowledgeLargeSchema
                  ? t("tipDisabledLargeSchema")
                  : undefined
            }
            className="w-full text-xs"
          >
            {loading ? (
              <Spinner size="xs" className="mr-1.5" />
            ) : (
              <HugeiconsIcon icon={MagicWand01Icon} className="mr-1.5 h-3.5 w-3.5" size="100%" />
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
          setProject={setProject}
          setOntology={setOntology}
          onApiError={onApiError}
          onRedesign={handleDesign}
          analysisRef={analysisRef}
          extend={form.extend}
          reanalyze={form.reanalyze}
        />
      )}

      {/* Designed: bridge to completed */}
      {isDesigned && !isCompleted && (
        <div className="space-y-2 rounded-lg border border-emerald-200 bg-emerald-50/50 p-3 dark:border-emerald-900 dark:bg-emerald-950/20">
          <h4 className="text-xs font-semibold text-emerald-800 dark:text-emerald-200">
            {t("finalize")}
          </h4>
          <FormInput
            type="text"
            placeholder={t("ontologyNamePlaceholder")}
            value={form.complete.completeName}
            onChange={(e) => form.complete.setCompleteName(e.target.value)}
          />
          <label className="flex items-center gap-2 text-xs text-zinc-600 dark:text-muted-foreground">
            <input
              type="checkbox"
              checked={form.complete.deployOnComplete}
              onChange={(e) => form.complete.setDeployOnComplete(e.target.checked)}
              className="h-3.5 w-3.5 rounded border-zinc-300 text-emerald-600"
            />
            {t("deployOnComplete")}
          </label>
          <Button
            size="sm"
            onClick={() => handleComplete()}
            disabled={loading || !form.complete.completeName.trim()}
            className="w-full text-xs"
          >
            {loading ? (
              <Spinner size="xs" className="mr-1.5" />
            ) : (
              <HugeiconsIcon icon={Tick01Icon} className="mr-1.5 h-3 w-3" size="100%" />
            )}
            {t("complete")}
          </Button>
        </div>
      )}

      {/* Completed status */}
      {isCompleted && (
        <div className="space-y-2 rounded-lg border border-emerald-200 bg-emerald-50/50 p-3 dark:border-emerald-900 dark:bg-emerald-950/20">
          <div className="flex items-center gap-2">
            <HugeiconsIcon icon={Tick01Icon} className="h-4 w-4 text-emerald-600" size="100%" />
            <h4 className="text-xs font-semibold text-emerald-800 dark:text-emerald-200">
              {t("savedHeader")}
            </h4>
          </div>
          <p className="text-[10px] text-emerald-700 dark:text-emerald-400">
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

      {isCompleted && project.ontology_id && (
        <GraphAuditSection ontologyId={project.ontology_id} />
      )}
    </>
  );
}
