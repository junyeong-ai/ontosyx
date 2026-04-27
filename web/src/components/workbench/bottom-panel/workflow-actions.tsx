"use client";

import { useState } from "react";
import { useTranslations } from "next-intl";
import { HugeiconsIcon } from "@hugeicons/react";
import {
  Tick01Icon,
  Refresh01Icon,
  Delete01Icon,
  MagicWand01Icon,
  Add01Icon,
} from "@hugeicons/core-free-icons";
import { Spinner } from "@/components/ui/spinner";
import { useAppStore } from "@/lib/store";
import {
  ApiError,
  designProjectStream,
  reanalyzeProject,
  completeProject,
  updateDecisions,
  deleteProject,
  extendProject,
  deploySchema,
  generateLoadPlan,
  compileLoad,
  auditGraph,
  adoptGraph,
} from "@/lib/api";
import type { GraphAuditReport } from "@/lib/api";
import { isGitUrl } from "@/lib/git-url";
import { Button } from "@/components/ui/button";
import { FormInput } from "@/components/ui/form-input";
import { useConfirm } from "@/components/ui/confirm-dialog";
import { useGuardPendingEdits } from "@/lib/guard-pending-edits";
import { toast } from "sonner";
import { errorMessage } from "@/lib/error-messages";
import type {
  DesignOptions,
  DesignProject,
  DesignSource,
  OntologyIR,
} from "@/types/api";
import {
  StatusBadge,
  relationshipKey,
  columnKey,
} from "./design-panel-shared";
import { ReconcileReportPanel } from "./reconcile-report-panel";
import { ReanalyzeForm, ExtendSourceForm } from "./workflow-forms";
import { ProgressIndicator, SourceHistorySection } from "./workflow-indicators";
import { useWorkflowFormState } from "./use-workflow-form-state";
import type { DesignDecisions } from "./use-design-decisions";
import { arr } from "@/lib/ir-collections";

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Left panel: project actions (analyzed -> design, designed -> enhance/complete)
// ---------------------------------------------------------------------------

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
  const lastReconcileReport = useAppStore((s) => s.lastReconcileReport);
  const setLastReconcileReport = useAppStore((s) => s.setLastReconcileReport);
  const guardPendingEdits = useGuardPendingEdits();
  const confirmDialog = useConfirm();

  const form = useWorkflowFormState(project.id, project.title, project.source_config.schema_name);
  const hasLargeSchema = (report?.schema_stats?.table_count ?? 0) > 100;
  const reanalyzeSourceType = project.source_config.source_type;

  // ---------------------------------------------------------------------------
  // Derived values
  // ---------------------------------------------------------------------------

  const isCompleted = project.status === "completed";
  const isDesigned = project.status === "designed";

  const canDesign =
    !loading &&
    !isCompleted &&
    unresolvedClarificationCount === 0 &&
    !needsPartialAcknowledgement;

  // ---------------------------------------------------------------------------
  // Handlers
  // ---------------------------------------------------------------------------

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
        toast.error(t("designFailed"), { description: errorMessage(streamErrorType, streamErrorMsg) });
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
          const msg = deployErr instanceof ApiError ? deployErr.message : t("toast.unknownError");
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

  async function handleReanalyze() {
    if (!(await guardPendingEdits(t("guardActions.reanalyze")))) return;
    let source: DesignSource;
    if (reanalyzeSourceType === "postgresql") {
      if (!form.reanalyze.connectionString.trim()) {
        toast.error(t("connectionStringRequired"));
        return;
      }
      source = {
        type: "postgresql",
        connection_string: form.reanalyze.connectionString.trim(),
        schema: form.reanalyze.schemaName.trim() || "public",
      };
    } else if (reanalyzeSourceType === "code_repository") {
      if (!form.reanalyze.repoUrl.trim()) {
        toast.error(t("repoUrlRequired"));
        return;
      }
      source = { type: "code_repository", url: form.reanalyze.repoUrl.trim() };
    } else {
      if (!form.reanalyze.sampleData.trim()) {
        toast.error(t("sourceDataRequired"));
        return;
      }
      source = { type: reanalyzeSourceType as "text" | "csv" | "json", data: form.reanalyze.sampleData.trim() };
    }

    setLoading(true);
    try {
      const resp = await reanalyzeProject(project.id, {
        source,
        revision: project.revision,
        repo_source: form.reanalyze.repoPath.trim()
          ? isGitUrl(form.reanalyze.repoPath.trim())
            ? { type: "git_url" as const, url: form.reanalyze.repoPath.trim() }
            : { type: "local" as const, path: form.reanalyze.repoPath.trim() }
          : undefined,
      });
      setProject(resp.project);
      form.reanalyze.setShowReanalyze(false);
      toast.success(t("reanalyzed"), {
        description: resp.invalidated_decisions?.length
          ? t("reanalyzedDescription", { count: resp.invalidated_decisions.length })
          : undefined,
      });
    } catch (err) {
      if (await onApiError(err, t("reanalyzeFailed"))) return;
    } finally {
      setLoading(false);
    }
  }

  async function handleExtend() {
    if (!(await guardPendingEdits(t("guardActions.extend")))) return;
    let source: DesignSource;
    if (form.extend.sourceType === "postgresql") {
      if (!form.extend.connectionString.trim()) {
        toast.error(t("connectionStringRequired"));
        return;
      }
      source = {
        type: "postgresql",
        connection_string: form.extend.connectionString.trim(),
        schema: form.extend.schemaName.trim() || "public",
      };
    } else if (form.extend.sourceType === "mysql") {
      if (!form.extend.connectionString.trim()) {
        toast.error(t("connectionStringRequired"));
        return;
      }
      if (!form.extend.database.trim()) {
        toast.error(t("databaseRequired"));
        return;
      }
      source = {
        type: "mysql",
        connection_string: form.extend.connectionString.trim(),
        schema: form.extend.database.trim(),
      };
    } else if (form.extend.sourceType === "mongodb") {
      if (!form.extend.connectionString.trim()) {
        toast.error(t("connectionStringRequired"));
        return;
      }
      if (!form.extend.database.trim()) {
        toast.error(t("databaseRequired"));
        return;
      }
      source = {
        type: "mongodb",
        connection_string: form.extend.connectionString.trim(),
        database: form.extend.database.trim(),
      };
    } else if (form.extend.sourceType === "duckdb") {
      if (!form.extend.duckdbFilePath.trim()) {
        toast.error(t("filePathRequired"));
        return;
      }
      source = { type: "duckdb", file_path: form.extend.duckdbFilePath.trim() };
    } else if (form.extend.sourceType === "snowflake") {
      toast.error(t("snowflakeExtendUnsupported"));
      return;
    } else if (form.extend.sourceType === "bigquery") {
      toast.error(t("bigqueryExtendUnsupported"));
      return;
    } else if (form.extend.sourceType === "code_repository") {
      if (!form.extend.repoUrl.trim()) {
        toast.error(t("repoUrlRequired"));
        return;
      }
      source = { type: "code_repository", url: form.extend.repoUrl.trim() };
    } else {
      if (!form.extend.sampleData.trim()) {
        toast.error(t("sourceDataRequired"));
        return;
      }
      source = { type: form.extend.sourceType, data: form.extend.sampleData.trim() };
    }

    setLoading(true);
    try {
      const resp = await extendProject(project.id, {
        revision: project.revision,
        source,
      });
      setProject(resp.project);
      if (resp.project.ontology) {
        setOntology(resp.project.ontology as OntologyIR);
      }
      setLastReconcileReport(resp.reconcile_report);
      form.extend.setShowExtend(false);
      toast.success(t("extendSuccess"));
      if (analysisRef.current) analysisRef.current.open = true;
    } catch (err) {
      if (await onApiError(err, t("extendFailed"))) return;
    } finally {
      setLoading(false);
    }
  }

  // ---------------------------------------------------------------------------
  // Schema Deploy & Load handlers
  // ---------------------------------------------------------------------------

  async function handleDeployPreview() {
    setLoading(true);
    try {
      const resp = await deploySchema(project.id, { dry_run: true });
      form.deploy.setDeployPreview(resp.statements);
    } catch (err) {
      if (await onApiError(err, t("deployPreviewFailed"))) return;
    } finally {
      setLoading(false);
    }
  }

  async function handleDeployExecute(skipConfirm = false) {
    if (!skipConfirm && !form.deploy.deployPreview) {
      const ok = await confirmDialog({
        title: t("deployConfirmTitle"),
        description: t("deployConfirmDescription"),
        confirmLabel: t("deployConfirmLabel"),
        variant: "warning",
      });
      if (!ok) return;
    }
    setLoading(true);
    try {
      const resp = await deploySchema(project.id, { dry_run: false });
      form.deploy.setDeployPreview(null);
      toast.success(t("schemaDeployed", { count: resp.statements.length }));
    } catch (err) {
      if (await onApiError(err, t("schemaDeployFailed"))) return;
    } finally {
      setLoading(false);
    }
  }

  async function handleGenerateLoadPlan() {
    setLoading(true);
    try {
      const resp = await generateLoadPlan(project.id);
      form.deploy.setLoadPlan(resp.plan);
    } catch (err) {
      if (await onApiError(err, t("loadPlanFailed"))) return;
    } finally {
      setLoading(false);
    }
  }

  async function handleCompileLoad() {
    if (!form.deploy.loadPlan) return;
    setLoading(true);
    try {
      const resp = await compileLoad(project.id, { plan: form.deploy.loadPlan });
      toast.success(t("loadCompiled", { count: resp.statements.length }));
    } catch (err) {
      if (await onApiError(err, t("loadCompileFailed"))) return;
    } finally {
      setLoading(false);
    }
  }

  // ---------------------------------------------------------------------------
  // Render
  // ---------------------------------------------------------------------------

  return (
    <>
      {/* Project header */}
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
              // Clear ontology from canvas
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

      {/* Source history */}
      {project.source_history?.length > 0 && (
        <SourceHistorySection entries={project.source_history} />
      )}

      {/* Progress indicator */}
      {loading && progressPhase && (
        <ProgressIndicator phase={progressPhase} detail={progressDetail} />
      )}

      {/* Analyzed state: Design is the primary action */}
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
              !canDesign &&
              (unresolvedClarificationCount > 0 || needsPartialAcknowledgement)
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
            <Button variant="outline" size="sm" onClick={handleSaveDecisions} disabled={loading} className="w-full text-xs">
              {t("saveDecisions")}
            </Button>
          )}
        </>
      )}

      {/* Designed/Completed state: Enhance & Finalize */}
      {(isDesigned || isCompleted) && (
        <>
          {/* Enhance section */}
          <div className="space-y-1.5">
            <p className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">{t("enhanceHeader")}</p>
            <p className="text-[10px] text-muted-foreground">
              {t.rich("enhanceHint", {
                kbd: (chunks) => (
                  <kbd className="rounded bg-zinc-200 px-1 py-0.5 font-mono text-[9px] dark:bg-zinc-700">
                    {chunks}
                  </kbd>
                ),
              })}
            </p>
            <Button
              variant="outline"
              size="sm"
              onClick={() => form.extend.setShowExtend(!form.extend.showExtend)}
              disabled={loading}
              className="w-full text-xs"
            >
              <HugeiconsIcon icon={Add01Icon} className="mr-1.5 h-3 w-3" size="100%" />
              {form.extend.showExtend ? tCommon("cancel") : t("extendWithSource")}
            </Button>
            {form.extend.showExtend && (
              <ExtendSourceForm
                sourceType={form.extend.sourceType}
                setSourceType={form.extend.setSourceType}
                connectionString={form.extend.connectionString}
                setConnectionString={form.extend.setConnectionString}
                schemaName={form.extend.schemaName}
                setSchemaName={form.extend.setSchemaName}
                database={form.extend.database}
                setDatabase={form.extend.setDatabase}
                sampleData={form.extend.sampleData}
                setSampleData={form.extend.setSampleData}
                repoUrl={form.extend.repoUrl}
                setRepoUrl={form.extend.setRepoUrl}
                duckdbFilePath={form.extend.duckdbFilePath}
                setDuckdbFilePath={form.extend.setDuckdbFilePath}
                loading={loading}
                onSubmit={handleExtend}
              />
            )}
          </div>

          {/* Reconcile report */}
          {lastReconcileReport && (
            <ReconcileReportPanel
              report={lastReconcileReport}
              onDismiss={() => { setLastReconcileReport(null); useAppStore.getState().setPendingReconcile(null); }}
            />
          )}

          {/* Advanced section (collapsed) */}
          <details className="text-xs">
            <summary className="cursor-pointer text-[10px] font-semibold uppercase tracking-wider text-muted-foreground hover:text-zinc-600 dark:hover:text-zinc-300">
              {t("advanced")}
            </summary>
            <div className="mt-2 space-y-2">
              <Button variant="outline" size="sm" onClick={handleDesign} disabled={loading} className="w-full text-xs">
                <HugeiconsIcon icon={MagicWand01Icon} className="mr-1.5 h-3 w-3" size="100%" />
                {t("redesign")}
              </Button>
              {reanalyzeSourceType !== "ontology" && (
                <>
                  <Button variant="outline" size="sm" onClick={() => form.reanalyze.setShowReanalyze(!form.reanalyze.showReanalyze)} disabled={loading} className="w-full text-xs">
                    <HugeiconsIcon icon={Refresh01Icon} className="mr-1.5 h-3 w-3" size="100%" />
                    {form.reanalyze.showReanalyze ? tCommon("cancel") : t("reanalyzeSource")}
                  </Button>
                  {form.reanalyze.showReanalyze && (
                    <ReanalyzeForm
                      sourceType={reanalyzeSourceType}
                      connectionString={form.reanalyze.connectionString}
                      setConnectionString={form.reanalyze.setConnectionString}
                      schemaName={form.reanalyze.schemaName}
                      setSchemaName={form.reanalyze.setSchemaName}
                      sampleData={form.reanalyze.sampleData}
                      setSampleData={form.reanalyze.setSampleData}
                      repoPath={form.reanalyze.repoPath}
                      setRepoPath={form.reanalyze.setRepoPath}
                      repoUrl={form.reanalyze.repoUrl}
                      setRepoUrl={form.reanalyze.setRepoUrl}
                      loading={loading}
                      onSubmit={handleReanalyze}
                    />
                  )}
                </>
              )}
            </div>
          </details>
        </>
      )}

      {/* Complete */}
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

      {/* Completed state */}
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

      {/* Schema Deployment */}
      {isCompleted && project.ontology && (
        <div className="space-y-2 rounded-lg border border-blue-200 bg-blue-50/50 p-3 dark:border-blue-900 dark:bg-blue-950/20">
          <h4 className="text-xs font-semibold text-blue-800 dark:text-blue-200">
            {t("schemaDeployment")}
          </h4>
          {form.deploy.deployPreview ? (
            <div className="space-y-2">
              <p className="text-[10px] text-blue-700 dark:text-blue-400">
                {t("ddlStatements", {
                  count: form.deploy.deployPreview.length,
                  plural: form.deploy.deployPreview.length !== 1 ? t("ddlStatementsPlural") : "",
                })}
              </p>
              <pre className="max-h-32 overflow-auto rounded bg-zinc-900 p-2 text-[10px] text-zinc-300">
                {form.deploy.deployPreview.join(";\n")}
              </pre>
              <div className="flex gap-2">
                <Button
                  size="sm"
                  onClick={() => handleDeployExecute(true)}
                  disabled={loading}
                  className="text-xs"
                >
                  {t("execute")}
                </Button>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => form.deploy.setDeployPreview(null)}
                  className="text-xs"
                >
                  {tCommon("cancel")}
                </Button>
              </div>
            </div>
          ) : (
            <div className="flex gap-2">
              <Button
                variant="outline"
                size="sm"
                onClick={handleDeployPreview}
                disabled={loading}
                className="flex-1 text-xs"
              >
                {t("previewDdl")}
              </Button>
              <Button
                size="sm"
                onClick={() => handleDeployExecute()}
                disabled={loading}
                className="flex-1 text-xs"
              >
                {t("deployToNeo4j")}
              </Button>
            </div>
          )}
        </div>
      )}

      {/* Graph Audit & Sync */}
      {isCompleted && project.ontology_id && (
        <GraphAuditSection ontologyId={project.ontology_id} />
      )}

      {/* Load Data — object_mappings live on the ontology itself now,
          so a completed project with an ontology always has mapping
          state. */}
      {isCompleted && project.ontology && (
        <div className="space-y-2 rounded-lg border border-purple-200 bg-purple-50/50 p-3 dark:border-purple-900 dark:bg-purple-950/20">
          <h4 className="text-xs font-semibold text-purple-800 dark:text-purple-200">
            {t("dataLoading")}
          </h4>
          {form.deploy.loadPlan ? (
            <div className="space-y-2">
              <p className="text-[10px] text-purple-700 dark:text-purple-400">
                {t("loadSteps", {
                  count: form.deploy.loadPlan.steps.length,
                  plural: form.deploy.loadPlan.steps.length !== 1 ? t("ddlStatementsPlural") : "",
                })}
              </p>
              <div className="space-y-1">
                {form.deploy.loadPlan.steps.map((step, i) => (
                  <div key={i} className="rounded bg-zinc-100 px-2 py-1 text-[10px] text-zinc-700 dark:bg-zinc-800 dark:text-zinc-300">
                    {t("loadStepItem", { order: step.order + 1, description: step.description })}
                  </div>
                ))}
              </div>
              <div className="flex gap-2">
                <Button
                  size="sm"
                  onClick={handleCompileLoad}
                  disabled={loading}
                  className="text-xs"
                >
                  {t("compileDdl")}
                </Button>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => form.deploy.setLoadPlan(null)}
                  className="text-xs"
                >
                  {tCommon("cancel")}
                </Button>
              </div>
            </div>
          ) : (
            <Button
              variant="outline"
              size="sm"
              onClick={handleGenerateLoadPlan}
              disabled={loading}
              className="w-full text-xs"
            >
              {t("generateLoadPlan")}
            </Button>
          )}
        </div>
      )}
    </>
  );
}

// ---------------------------------------------------------------------------
// Graph Audit & Sync Section
// ---------------------------------------------------------------------------

function GraphAuditSection({ ontologyId }: { ontologyId: string }) {
  const t = useTranslations("workbench.bottomPanel.workflowActions");
  const [report, setReport] = useState<GraphAuditReport | null>(null);
  const [loading, setLoading] = useState(false);
  const [adopting, setAdopting] = useState(false);
  const { setOntology } = useAppStore();

  const handleAudit = async () => {
    setLoading(true);
    try {
      const result = await auditGraph(ontologyId);
      setReport(result);
    } catch (e) {
      toast.error(e instanceof ApiError ? e.message : t("auditFailed"));
    } finally {
      setLoading(false);
    }
  };

  const handleAdopt = async () => {
    setAdopting(true);
    try {
      const adopted = await adoptGraph(t("adoptedName"), true);
      setOntology(adopted);
      toast.success(
        t("adopted", {
          nodeCount: arr(adopted.node_types).length,
          edgeCount: arr(adopted.edge_types).length,
        }),
      );
      // Re-audit to confirm sync
      const result = await auditGraph(ontologyId);
      setReport(result);
    } catch (e) {
      toast.error(e instanceof ApiError ? e.message : t("adoptFailed"));
    } finally {
      setAdopting(false);
    }
  };

  const syncColor = report?.sync_status === "synced"
    ? "emerald" : report?.sync_status === "partial"
    ? "amber" : "red";

  return (
    <div className="space-y-2 rounded-lg border border-teal-200 bg-teal-50/50 p-3 dark:border-teal-900 dark:bg-teal-950/20">
      <h4 className="text-xs font-semibold text-teal-800 dark:text-teal-200">
        {t("graphSync")}
      </h4>

      {!report ? (
        <div className="space-y-2">
          <p className="text-[10px] text-teal-700 dark:text-teal-400">
            {t("graphSyncDescription")}
          </p>
          <Button size="sm" onClick={handleAudit} disabled={loading}>
            {loading ? <Spinner size="xs" /> : <HugeiconsIcon icon={Refresh01Icon} className="mr-1 h-3 w-3" />}
            {t("auditGraph")}
          </Button>
        </div>
      ) : (
        <div className="space-y-2">
          {/* Sync status badge */}
          <div className="flex items-center gap-2">
            <span className={`rounded-full px-2 py-0.5 text-[10px] font-medium ${
              syncColor === "emerald" ? "bg-emerald-100 text-emerald-700 dark:bg-emerald-900 dark:text-emerald-300" :
              syncColor === "amber" ? "bg-amber-100 text-amber-700 dark:bg-amber-900 dark:text-amber-300" :
              "bg-red-100 text-red-700 dark:bg-red-900 dark:text-red-300"
            }`}>
              {report.sync_status === "synced"
                ? t("syncStatusSynced")
                : report.sync_status === "partial"
                  ? t("syncStatusPartial")
                  : t("syncStatusUnsynced")}
            </span>
            <span className="text-[10px] text-muted-foreground">
              {t("syncPercentage", { percent: report.sync_percentage })}
            </span>
          </div>

          {/* Matched */}
          {report.matched_nodes.length > 0 && (
            <p className="text-[10px] text-emerald-600 dark:text-emerald-400">
              {t("syncMatched", {
                nodeCount: report.matched_nodes.length,
                edgeCount: report.matched_edges.length,
              })}
            </p>
          )}

          {/* Orphan graph labels (in graph but not in ontology) */}
          {report.orphan_graph_edges.length > 0 && (
            <details className="text-[10px]">
              <summary className="cursor-pointer text-amber-600 dark:text-amber-400">
                {t("orphanGraphEdges", { count: report.orphan_graph_edges.length })}
              </summary>
              <div className="mt-1 flex flex-wrap gap-1">
                {report.orphan_graph_edges.map((e) => (
                  <span key={e} className="rounded bg-amber-100 px-1.5 py-0.5 text-amber-700 dark:bg-amber-900 dark:text-amber-300">
                    {e}
                  </span>
                ))}
              </div>
            </details>
          )}

          {/* Missing graph labels (in ontology but not in graph) */}
          {report.missing_graph_edges.length > 0 && (
            <details className="text-[10px]">
              <summary className="cursor-pointer text-red-600 dark:text-red-400">
                {t("missingGraphEdges", { count: report.missing_graph_edges.length })}
              </summary>
              <div className="mt-1 flex flex-wrap gap-1">
                {report.missing_graph_edges.map((e) => (
                  <span key={e} className="rounded bg-red-100 px-1.5 py-0.5 text-red-700 dark:bg-red-900 dark:text-red-300">
                    {e}
                  </span>
                ))}
              </div>
            </details>
          )}

          {/* Actions */}
          <div className="flex gap-2">
            <Button size="sm" variant="ghost" onClick={handleAudit} disabled={loading}>
              {loading ? <Spinner size="xs" /> : t("reaudit")}
            </Button>
            {report.sync_status !== "synced" && (
              <Button size="sm" onClick={handleAdopt} disabled={adopting}>
                {adopting ? <Spinner size="xs" /> : <HugeiconsIcon icon={MagicWand01Icon} className="mr-1 h-3 w-3" />}
                {t("adoptGraphLabels")}
              </Button>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
