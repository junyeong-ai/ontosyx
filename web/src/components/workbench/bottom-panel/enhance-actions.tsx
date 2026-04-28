"use client";

import { useTranslations } from "next-intl";
import { HugeiconsIcon } from "@hugeicons/react";
import { Add01Icon, MagicWand01Icon, Refresh01Icon } from "@hugeicons/core-free-icons";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { toAnalyzeSelection } from "@/components/workbench/source-import-panel";
import { useAppStore } from "@/lib/store";
import { extendProject, reanalyzeProject } from "@/lib/api";
import { isGitUrl } from "@/lib/git-url";
import { useGuardPendingEdits } from "@/lib/guard-pending-edits";
import type { DesignProject, DesignSource, OntologyIR } from "@/types/api";

import { ReanalyzeForm, ExtendSourceForm } from "./workflow-forms";
import { ReconcileReportPanel } from "./reconcile-report-panel";
import type { useWorkflowFormState } from "./use-workflow-form-state";

type FormState = ReturnType<typeof useWorkflowFormState>;

export interface EnhanceActionsProps {
  project: DesignProject;
  loading: boolean;
  setLoading: (v: boolean) => void;
  setProject: (p: DesignProject | null) => void;
  setOntology: (o: OntologyIR) => void;
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
  setProject,
  setOntology,
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
  const reanalyzeSourceType = project.source_config.source_type;

  async function handleExtend() {
    if (!(await guardPendingEdits(t("guardActions.extend")))) return;
    let source: DesignSource;
    if (extend.sourceType === "postgresql") {
      if (!extend.connectionString.trim()) {
        toast.error(t("connectionStringRequired"));
        return;
      }
      source = {
        type: "postgresql",
        connection_string: extend.connectionString.trim(),
        schema: extend.schemaName.trim() || "public",
      };
    } else if (extend.sourceType === "mysql") {
      if (!extend.connectionString.trim()) {
        toast.error(t("connectionStringRequired"));
        return;
      }
      if (!extend.database.trim()) {
        toast.error(t("databaseRequired"));
        return;
      }
      source = {
        type: "mysql",
        connection_string: extend.connectionString.trim(),
        schema: extend.database.trim(),
      };
    } else if (extend.sourceType === "mongodb") {
      if (!extend.connectionString.trim()) {
        toast.error(t("connectionStringRequired"));
        return;
      }
      if (!extend.database.trim()) {
        toast.error(t("databaseRequired"));
        return;
      }
      source = {
        type: "mongodb",
        connection_string: extend.connectionString.trim(),
        database: extend.database.trim(),
      };
    } else if (extend.sourceType === "duckdb") {
      if (!extend.duckdbFilePath.trim()) {
        toast.error(t("filePathRequired"));
        return;
      }
      source = { type: "duckdb", file_path: extend.duckdbFilePath.trim() };
    } else if (extend.sourceType === "snowflake") {
      toast.error(t("snowflakeExtendUnsupported"));
      return;
    } else if (extend.sourceType === "bigquery") {
      toast.error(t("bigqueryExtendUnsupported"));
      return;
    } else if (extend.sourceType === "code_repository") {
      if (!extend.repoUrl.trim()) {
        toast.error(t("repoUrlRequired"));
        return;
      }
      source = { type: "code_repository", url: extend.repoUrl.trim() };
    } else {
      if (!extend.sampleData.trim()) {
        toast.error(t("sourceDataRequired"));
        return;
      }
      source = { type: extend.sourceType, data: extend.sampleData.trim() };
    }

    setLoading(true);
    try {
      const resp = await extendProject(project.id, {
        revision: project.revision,
        source,
        // The Design-mode "Import Tables" flow always lowers
        // `subset` to `extend` so the existing project absorbs
        // only the picked tables.
        selection: toAnalyzeSelection(extend.importValue, "extend"),
      });
      setProject(resp.project);
      if (resp.project.ontology) {
        setOntology(resp.project.ontology as OntologyIR);
      }
      setLastReconcileReport(resp.reconcile_report);
      extend.setShowExtend(false);
      toast.success(t("extendSuccess"));
      if (analysisRef.current) analysisRef.current.open = true;
    } catch (err) {
      if (await onApiError(err, t("extendFailed"))) return;
    } finally {
      setLoading(false);
    }
  }

  async function handleReanalyze() {
    if (!(await guardPendingEdits(t("guardActions.reanalyze")))) return;
    let source: DesignSource;
    if (reanalyzeSourceType === "postgresql") {
      if (!reanalyze.connectionString.trim()) {
        toast.error(t("connectionStringRequired"));
        return;
      }
      source = {
        type: "postgresql",
        connection_string: reanalyze.connectionString.trim(),
        schema: reanalyze.schemaName.trim() || "public",
      };
    } else if (reanalyzeSourceType === "code_repository") {
      if (!reanalyze.repoUrl.trim()) {
        toast.error(t("repoUrlRequired"));
        return;
      }
      source = { type: "code_repository", url: reanalyze.repoUrl.trim() };
    } else {
      if (!reanalyze.sampleData.trim()) {
        toast.error(t("sourceDataRequired"));
        return;
      }
      source = {
        type: reanalyzeSourceType as "text" | "csv" | "json",
        data: reanalyze.sampleData.trim(),
      };
    }

    setLoading(true);
    try {
      const resp = await reanalyzeProject(project.id, {
        source,
        revision: project.revision,
        repo_source: reanalyze.repoPath.trim()
          ? isGitUrl(reanalyze.repoPath.trim())
            ? { type: "git_url" as const, url: reanalyze.repoPath.trim() }
            : { type: "local" as const, path: reanalyze.repoPath.trim() }
          : undefined,
        // Reanalyze defaults to a full sweep — narrowing requires
        // a UI for selection, which lives only on the extend
        // surface today. Sent explicitly so the wire is self-
        // describing.
        selection: { kind: "all" },
      });
      setProject(resp.project);
      reanalyze.setShowReanalyze(false);
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

  return (
    <>
      <div className="space-y-1.5">
        <p className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
          {t("enhanceHeader")}
        </p>
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
          onClick={() => extend.setShowExtend(!extend.showExtend)}
          disabled={loading}
          className="w-full text-xs"
        >
          <HugeiconsIcon icon={Add01Icon} className="mr-1.5 h-3 w-3" size="100%" />
          {extend.showExtend ? tCommon("cancel") : t("extendWithSource")}
        </Button>
        {extend.showExtend && (
          <ExtendSourceForm
            sourceType={extend.sourceType}
            setSourceType={extend.setSourceType}
            connectionString={extend.connectionString}
            setConnectionString={extend.setConnectionString}
            schemaName={extend.schemaName}
            setSchemaName={extend.setSchemaName}
            database={extend.database}
            setDatabase={extend.setDatabase}
            sampleData={extend.sampleData}
            setSampleData={extend.setSampleData}
            repoUrl={extend.repoUrl}
            setRepoUrl={extend.setRepoUrl}
            duckdbFilePath={extend.duckdbFilePath}
            setDuckdbFilePath={extend.setDuckdbFilePath}
            importValue={extend.importValue}
            setImportValue={extend.setImportValue}
            loading={loading}
            onSubmit={handleExtend}
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
        <summary className="cursor-pointer text-[10px] font-semibold uppercase tracking-wider text-muted-foreground hover:text-zinc-600 dark:hover:text-zinc-300">
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
            <HugeiconsIcon icon={MagicWand01Icon} className="mr-1.5 h-3 w-3" size="100%" />
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
                <HugeiconsIcon icon={Refresh01Icon} className="mr-1.5 h-3 w-3" size="100%" />
                {reanalyze.showReanalyze ? tCommon("cancel") : t("reanalyzeSource")}
              </Button>
              {reanalyze.showReanalyze && (
                <ReanalyzeForm
                  sourceType={reanalyzeSourceType}
                  connectionString={reanalyze.connectionString}
                  setConnectionString={reanalyze.setConnectionString}
                  schemaName={reanalyze.schemaName}
                  setSchemaName={reanalyze.setSchemaName}
                  sampleData={reanalyze.sampleData}
                  setSampleData={reanalyze.setSampleData}
                  repoPath={reanalyze.repoPath}
                  setRepoPath={reanalyze.setRepoPath}
                  repoUrl={reanalyze.repoUrl}
                  setRepoUrl={reanalyze.setRepoUrl}
                  loading={loading}
                  onSubmit={handleReanalyze}
                />
              )}
            </>
          )}
        </div>
      </details>
    </>
  );
}
