"use client";

import { useTranslations } from "next-intl";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { useConfirm } from "@/components/providers/confirm-provider";
import { compileLoad, deploySchema, generateLoadPlan } from "@/lib/api";
import type { LoadPlan } from "@/types/api";

export interface DeploymentActionsProps {
  projectId: string;
  loading: boolean;
  setLoading: (v: boolean) => void;
  onApiError: (err: unknown, label: string) => Promise<boolean>;
  deployPreview: string[] | null;
  setDeployPreview: (v: string[] | null) => void;
  loadPlan: LoadPlan | null;
  setLoadPlan: (v: LoadPlan | null) => void;
}

export function DeploymentActions({
  projectId,
  loading,
  setLoading,
  onApiError,
  deployPreview,
  setDeployPreview,
  loadPlan,
  setLoadPlan,
}: DeploymentActionsProps) {
  const t = useTranslations("workbench.bottomPanel.workflowActions");
  const tCommon = useTranslations("common");
  const confirmDialog = useConfirm();

  async function handleDeployPreview() {
    setLoading(true);
    try {
      const resp = await deploySchema(projectId, { dry_run: true });
      setDeployPreview(resp.statements);
    } catch (err) {
      if (await onApiError(err, t("deployPreviewFailed"))) return;
    } finally {
      setLoading(false);
    }
  }

  async function handleDeployExecute(skipConfirm = false) {
    if (!skipConfirm && !deployPreview) {
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
      const resp = await deploySchema(projectId, { dry_run: false });
      setDeployPreview(null);
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
      const resp = await generateLoadPlan(projectId);
      setLoadPlan(resp.plan);
    } catch (err) {
      if (await onApiError(err, t("loadPlanFailed"))) return;
    } finally {
      setLoading(false);
    }
  }

  async function handleCompileLoad() {
    if (!loadPlan) return;
    setLoading(true);
    try {
      const resp = await compileLoad(projectId, { plan: loadPlan });
      toast.success(t("loadCompiled", { count: resp.statements.length }));
    } catch (err) {
      if (await onApiError(err, t("loadCompileFailed"))) return;
    } finally {
      setLoading(false);
    }
  }

  return (
    <>
      {/* Schema Deployment */}
      <div className="space-y-2 rounded-lg border border-info-border bg-info-surface/50 p-3 dark:border-info-border">
        <h4 className="text-xs font-semibold text-info-foreground">
          {t("schemaDeployment")}
        </h4>
        {deployPreview ? (
          <div className="space-y-2">
            <p className="text-2xs text-info-foreground dark:text-info-foreground">
              {t("ddlStatements", {
                count: deployPreview.length,
                plural: deployPreview.length !== 1 ? t("ddlStatementsPlural") : "",
              })}
            </p>
            <pre className="max-h-32 overflow-auto rounded bg-surface-base p-2 text-2xs text-foreground-muted">
              {deployPreview.join(";\n")}
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
                onClick={() => setDeployPreview(null)}
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

      {/* Load Data */}
      <div className="space-y-2 rounded-lg border border-concept-border bg-concept-surface/50 p-3 dark:border-concept-border">
        <h4 className="text-xs font-semibold text-concept-foreground">
          {t("dataLoading")}
        </h4>
        {loadPlan ? (
          <div className="space-y-2">
            <p className="text-2xs text-concept-foreground dark:text-concept-foreground">
              {t("loadSteps", {
                count: loadPlan.steps.length,
                plural: loadPlan.steps.length !== 1 ? t("ddlStatementsPlural") : "",
              })}
            </p>
            <div className="space-y-1">
              {loadPlan.steps.map((step, i) => (
                <div
                  key={i}
                  className="rounded bg-surface-inset px-2 py-1 text-2xs text-foreground-muted"
                >
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
                onClick={() => setLoadPlan(null)}
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
    </>
  );
}
