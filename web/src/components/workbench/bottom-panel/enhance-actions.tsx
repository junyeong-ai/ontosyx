"use client";

import { useEffect, useRef } from "react";
import { useTranslations } from "next-intl";
import { HugeiconsIcon } from "@hugeicons/react";
import { Add01Icon, MagicWand01Icon, Refresh01Icon } from "@hugeicons/core-free-icons";
import { toast } from "@/components/ui/toast";
import { KeyboardShortcut } from "@/components/ui/keyboard-shortcut";

import { Button } from "@/components/ui/button";
import { toAnalyzeSelection } from "@/components/workbench/source-import-panel";
import { useAppStore } from "@/lib/store";
import {
  extendProject,
  reanalyzeModeledProject,
  reanalyzeProject,
} from "@/lib/api";
import { isGitUrl } from "@/lib/git-url";
import { useGuardPendingEdits } from "@/lib/guard-pending-edits";
import { useFormWithSchema } from "@/hooks/use-form-with-schema";
import type { DesignProject } from "@/types/api";

import { ReanalyzeForm, ExtendSourceForm } from "./workflow-forms";
import { ReconcileReportPanel } from "./reconcile-report-panel";
import {
  ExtendSourceFormSchema,
  ReanalyzeSourceFormSchema,
  buildExtendInput,
  buildReanalyzeInput,
  toDesignSource,
  type ValidatedSourceFormValue,
} from "./source-form-schema";
import type { useWorkflowFormState } from "./use-workflow-form-state";

type FormState = ReturnType<typeof useWorkflowFormState>;

export interface EnhanceActionsProps {
  project: DesignProject;
  loading: boolean;
  setLoading: (v: boolean) => void;
  /**
   * Atomic project + ontology cache update — see
   * `OntologySlice.applyProjectSnapshot`.
   */
  applyProjectSnapshot: (project: DesignProject | null) => void;
  onApiError: (err: unknown, label: string) => Promise<boolean>;
  onRedesign: () => Promise<void>;
  analysisRef: React.RefObject<HTMLDetailsElement | null>;
  extend: FormState["extend"];
  reanalyze: FormState["reanalyze"];
}

export function EnhanceActions({
  project,
  loading,
  setLoading,
  applyProjectSnapshot,
  onApiError,
  onRedesign,
  analysisRef,
  extend,
  reanalyze,
}: EnhanceActionsProps) {
  const t = useTranslations("workbench.bottomPanel.workflowActions");
  const tCommon = useTranslations("common");
  const guardPendingEdits = useGuardPendingEdits();
  const lastReconcileReport = useAppStore((s) => s.lastReconcileReport);
  const setLastReconcileReport = useAppStore((s) => s.setLastReconcileReport);
  const extendRequestCount = useAppStore((s) => s.extendSourceRequestCount);
  const reanalyzeSourceType = project.source_config.source_type;

  // Header / shortcut callers fire `requestExtendSource()` to auto-
  // open the extend sub-form here without prop-drilling. Track the
  // counter so a fresh request opens the form even when the user
  // already collapsed it.
  const lastSeenExtendRequest = useRef(extendRequestCount);
  useEffect(() => {
    if (extendRequestCount !== lastSeenExtendRequest.current) {
      lastSeenExtendRequest.current = extendRequestCount;
      extend.setShowExtend(true);
    }
  }, [extendRequestCount, extend]);

  // ---------------------------------------------------------------------------
  // Extend
  // ---------------------------------------------------------------------------

  const extendForm = useFormWithSchema({
    schema: ExtendSourceFormSchema,
    onValid: async (validated: ValidatedSourceFormValue) => {
      if (!(await guardPendingEdits(t("guardActions.extend")))) return;
      setLoading(true);
      try {
        const resp = await extendProject(project.id, {
          revision: project.revision,
          source: toDesignSource(validated),
          // Design-mode "Import Tables" always lowers `subset` to
          // `extend` so the existing project absorbs only the picked
          // tables.
          selection: toAnalyzeSelection(extend.importValue, "extend"),
        });
        applyProjectSnapshot(resp.project);
        setLastReconcileReport(resp.reconcile_report);
        extend.setShowExtend(false);
        toast.success(t("extendSuccess"));
        if (analysisRef.current) analysisRef.current.open = true;
      } catch (err) {
        if (await onApiError(err, t("extendFailed"))) return;
      } finally {
        setLoading(false);
      }
    },
  });

  function handleExtend() {
    void extendForm.submit(
      buildExtendInput(extend.sourceType, {
        connectionString: extend.connectionString,
        schemaName: extend.schemaName,
        database: extend.database,
        duckdbFilePath: extend.duckdbFilePath,
        repoUrl: extend.repoUrl,
        sampleData: extend.sampleData,
      }),
    );
  }

  // ---------------------------------------------------------------------------
  // Reanalyze
  // ---------------------------------------------------------------------------

  const reanalyzeForm = useFormWithSchema({
    schema: ReanalyzeSourceFormSchema,
    onValid: async (validated: ValidatedSourceFormValue) => {
      if (!(await guardPendingEdits(t("guardActions.reanalyze")))) return;
      setLoading(true);
      try {
        const repoPath = reanalyze.repoPath.trim();
        const repo_source = repoPath
          ? isGitUrl(repoPath)
            ? { type: "git_url" as const, url: repoPath }
            : { type: "local" as const, path: repoPath }
          : undefined;
        const source = toDesignSource(validated);
        const resp = reanalyze.modeledOnly
          ? await reanalyzeModeledProject(project.id, {
              source,
              revision: project.revision,
              repo_source,
            })
          : await reanalyzeProject(project.id, {
              source,
              revision: project.revision,
              repo_source,
              selection: { kind: "all" },
            });
        applyProjectSnapshot(resp.project);
        reanalyze.setShowReanalyze(false);
        toast.success(t("reanalyzed"), {
          description: resp.invalidated_decisions?.length
            ? t("reanalyzedDescription", {
                count: resp.invalidated_decisions.length,
              })
            : undefined,
        });
      } catch (err) {
        if (await onApiError(err, t("reanalyzeFailed"))) return;
      } finally {
        setLoading(false);
      }
    },
  });

  function handleReanalyze() {
    void reanalyzeForm.submit(
      buildReanalyzeInput(reanalyzeSourceType, {
        connectionString: reanalyze.connectionString,
        schemaName: reanalyze.schemaName,
        database: "",
        duckdbFilePath: "",
        repoUrl: reanalyze.repoUrl,
        sampleData: reanalyze.sampleData,
      }),
    );
  }

  return (
    <>
      <div className="space-y-1.5">
        <p className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
          {t("enhanceHeader")}
        </p>
        <p className="text-2xs text-foreground-muted">
          {t.rich("enhanceHint", {
            kbd: () => <KeyboardShortcut keys="mod+k" />,
          })}
        </p>
        <Button
          variant="outline"
          size="sm"
          onClick={() => extend.setShowExtend(!extend.showExtend)}
          disabled={loading}
          className="w-full text-xs"
        >
          <HugeiconsIcon icon={Add01Icon} className="me-1.5 h-3 w-3" size="100%" />
          {extend.showExtend ? tCommon("cancel") : t("extendWithSource")}
        </Button>
        {extend.showExtend && (
          <ExtendSourceForm
            sourceType={extend.sourceType}
            setSourceType={(next) => {
              extend.setSourceType(next);
              extendForm.clearErrors();
            }}
            connectionString={extend.connectionString}
            setConnectionString={(v) => {
              extend.setConnectionString(v);
              extendForm.clearErrors("connectionString");
            }}
            schemaName={extend.schemaName}
            setSchemaName={extend.setSchemaName}
            database={extend.database}
            setDatabase={(v) => {
              extend.setDatabase(v);
              extendForm.clearErrors("database");
            }}
            sampleData={extend.sampleData}
            setSampleData={(v) => {
              extend.setSampleData(v);
              extendForm.clearErrors("sampleData");
            }}
            repoUrl={extend.repoUrl}
            setRepoUrl={(v) => {
              extend.setRepoUrl(v);
              extendForm.clearErrors("repoUrl");
            }}
            duckdbFilePath={extend.duckdbFilePath}
            setDuckdbFilePath={(v) => {
              extend.setDuckdbFilePath(v);
              extendForm.clearErrors("duckdbFilePath");
            }}
            importValue={extend.importValue}
            setImportValue={extend.setImportValue}
            loading={loading}
            onSubmit={handleExtend}
            errors={extendForm.errors}
          />
        )}
      </div>

      {lastReconcileReport && (
        <ReconcileReportPanel
          report={lastReconcileReport}
          onDismiss={() => {
            setLastReconcileReport(null);
            useAppStore.getState().setPendingReconcile(null);
          }}
        />
      )}

      <details className="text-xs">
        <summary className="cursor-pointer text-2xs font-semibold uppercase tracking-wider text-foreground-muted hover:text-foreground-muted">
          {t("advanced")}
        </summary>
        <div className="mt-2 space-y-2">
          <Button
            variant="outline"
            size="sm"
            onClick={onRedesign}
            disabled={loading}
            className="w-full text-xs"
          >
            <HugeiconsIcon icon={MagicWand01Icon} className="me-1.5 h-3 w-3" size="100%" />
            {t("redesign")}
          </Button>
          {reanalyzeSourceType !== "ontology" && (
            <>
              <Button
                variant="outline"
                size="sm"
                onClick={() => reanalyze.setShowReanalyze(!reanalyze.showReanalyze)}
                disabled={loading}
                className="w-full text-xs"
              >
                <HugeiconsIcon icon={Refresh01Icon} className="me-1.5 h-3 w-3" size="100%" />
                {reanalyze.showReanalyze ? tCommon("cancel") : t("reanalyzeSource")}
              </Button>
              {reanalyze.showReanalyze && (
                <ReanalyzeForm
                  sourceType={reanalyzeSourceType}
                  connectionString={reanalyze.connectionString}
                  setConnectionString={(v) => {
                    reanalyze.setConnectionString(v);
                    reanalyzeForm.clearErrors("connectionString");
                  }}
                  schemaName={reanalyze.schemaName}
                  setSchemaName={reanalyze.setSchemaName}
                  sampleData={reanalyze.sampleData}
                  setSampleData={(v) => {
                    reanalyze.setSampleData(v);
                    reanalyzeForm.clearErrors("sampleData");
                  }}
                  repoPath={reanalyze.repoPath}
                  setRepoPath={reanalyze.setRepoPath}
                  repoUrl={reanalyze.repoUrl}
                  setRepoUrl={(v) => {
                    reanalyze.setRepoUrl(v);
                    reanalyzeForm.clearErrors("repoUrl");
                  }}
                  loading={loading}
                  onSubmit={handleReanalyze}
                  modeledOnly={reanalyze.modeledOnly}
                  setModeledOnly={reanalyze.setModeledOnly}
                  modeledTablesAvailable={
                    project.analysis_scope?.included?.length ?? 0
                  }
                  errors={reanalyzeForm.errors}
                />
              )}
            </>
          )}
        </div>
      </details>
    </>
  );
}
