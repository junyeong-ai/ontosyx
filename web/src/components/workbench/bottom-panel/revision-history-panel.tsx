"use client";

import { useState, useEffect } from "react";
import { useTranslations } from "next-intl";
import { Spinner } from "@/components/ui/spinner";
import { useAppStore } from "@/lib/store";
import { useConfirm } from "@/components/providers/confirm-provider";
import { cn } from "@/lib/cn";
import { toast } from "sonner";
import {
  ApiError,
  listRevisions,
  restoreRevision,
  getRevisionDiff,
  migrateSchema,
} from "@/lib/api";
import type {
  DesignProject,
  ProjectMigrateResponse,
  OntologyDiff,
  RevisionSummary,
} from "@/types/api";
import { DiffPanel } from "./diff-panel";

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

export interface RevisionHistoryPanelProps {
  project: DesignProject;
  loading: boolean;
  setLoading: (v: boolean) => void;
  /**
   * Atomic project + ontology cache update — see
   * `OntologySlice.applyProjectSnapshot`. Restore / migrate / fork
   * actions land their result through this single entry point so
   * `activeProject` and the ontology cache cannot drift.
   */
  applyProjectSnapshot: (project: DesignProject | null) => void;
  onApiError: (err: unknown, label: string) => Promise<boolean>;
}

// ---------------------------------------------------------------------------
// Revision History Panel
// ---------------------------------------------------------------------------

export function RevisionHistoryPanel({
  project,
  loading,
  setLoading,
  applyProjectSnapshot,
  onApiError,
}: RevisionHistoryPanelProps) {
  const t = useTranslations("workbench.bottomPanel.revision");
  const tCommon = useTranslations("common");
  const confirmDialog = useConfirm();
  const setActiveDiffOverlay = useAppStore((s) => s.setActiveDiffOverlay);

  // Revision list
  const [revisions, setRevisions] = useState<RevisionSummary[]>([]);
  const [revisionsLoading, setRevisionsLoading] = useState(false);

  // Migration
  const [migrationResult, setMigrationResult] = useState<ProjectMigrateResponse | null>(null);
  const [migrationTargetRev, setMigrationTargetRev] = useState<number | null>(null);

  // Diff comparison
  const [diffCompareBase, setDiffCompareBase] = useState<number | null>(null);
  const [diffResult, setDiffResult] = useState<OntologyDiff | null>(null);
  const [diffLabels, setDiffLabels] = useState<{ base: string; target: string }>({
    base: "",
    target: "",
  });
  const [diffLoading, setDiffLoading] = useState(false);

  // Reset state when project changes
  useEffect(() => {
    setRevisions([]);
    setMigrationResult(null);
    setMigrationTargetRev(null);
    setDiffResult(null);
    setDiffCompareBase(null);
  }, [project.id]);

  async function handleCompare(targetRevision: number) {
    if (diffCompareBase === null || diffCompareBase === targetRevision) return;
    setDiffLoading(true);
    try {
      const diff = await getRevisionDiff(project.id, diffCompareBase, targetRevision);
      setDiffResult(diff);
      setDiffLabels({
        base: t("baseLabel", { revision: diffCompareBase }),
        target: t("baseLabel", { revision: targetRevision }),
      });
      setDiffCompareBase(null);
      setActiveDiffOverlay(diff);
    } catch (e) {
      const msg = e instanceof ApiError ? e.message : t("diffFailed");
      toast.error(msg);
    } finally {
      setDiffLoading(false);
    }
  }

  function dismissDiff() {
    setDiffResult(null);
    setActiveDiffOverlay(null);
  }

  async function loadRevisions() {
    setRevisionsLoading(true);
    try {
      const data = await listRevisions(project.id);
      setRevisions(data);
    } catch {
      // Silently fail -- revision history is non-critical
    } finally {
      setRevisionsLoading(false);
    }
  }

  async function handleRestore(rev: number) {
    const ok = await confirmDialog({
      title: t("restoreTitle"),
      description: t("restoreDescription", { revision: rev }),
      confirmLabel: t("restoreConfirmLabel"),
      variant: "warning",
    });
    if (!ok) return;

    setLoading(true);
    try {
      const resp = await restoreRevision(project.id, rev);
      applyProjectSnapshot(resp.project);
      loadRevisions();
      toast.success(t("toast.restored", { revision: rev }));
    } catch (err) {
      if (await onApiError(err, t("restoreFailed"))) return;
    } finally {
      setLoading(false);
    }
  }

  async function handleMigrate(rev: number) {
    setLoading(true);
    try {
      const resp = await migrateSchema(project.id, rev, { dry_run: true });
      setMigrationResult(resp);
      setMigrationTargetRev(rev);
      if (resp.up.length === 0) {
        toast.info(t("noSchemaChanges"));
      }
    } catch (err) {
      if (await onApiError(err, t("migrationPreviewFailed"))) return;
    } finally {
      setLoading(false);
    }
  }

  async function handleExecuteMigration(rev: number) {
    setLoading(true);
    try {
      const resp = await migrateSchema(project.id, rev, { dry_run: false });
      setMigrationResult(null);
      toast.success(t("migrationSuccess", { count: resp.up.length }));
    } catch (err) {
      if (await onApiError(err, t("migrationFailed"))) return;
    } finally {
      setLoading(false);
    }
  }

  return (
    <details
      onToggle={(e) => {
        if ((e.target as HTMLDetailsElement).open && revisions.length === 0) {
          loadRevisions();
        }
      }}
    >
      <summary className="cursor-pointer text-xs font-semibold uppercase tracking-wider text-muted-foreground hover:text-foreground dark:hover:text-foreground-muted">
        {t("title")}
      </summary>
      <div className="mt-2">
        {/* Compare mode instructions */}
        {diffCompareBase !== null && (
          <div className="mb-2 flex items-center gap-2 rounded bg-info-surface px-2 py-1.5 text-[11px] text-info-foreground dark:text-info-foreground">
            <span>{t("compareBase", { revision: diffCompareBase })}</span>
            <button
              onClick={() => setDiffCompareBase(null)}
              className="ml-auto text-2xs font-medium text-info-foreground hover:text-info-foreground dark:text-info-foreground"
            >
              {tCommon("cancel")}
            </button>
          </div>
        )}
        {diffLoading && (
          <div className="mb-2 flex items-center gap-2 py-1 text-xs text-muted-foreground">
            <Spinner size="xs" /> {t("computing")}
          </div>
        )}
        {revisionsLoading ? (
          <div className="flex items-center gap-2 py-2 text-xs text-muted-foreground">
            <Spinner size="xs" /> {tCommon("loading")}
          </div>
        ) : revisions.length === 0 ? (
          <p className="py-2 text-xs text-muted-foreground">{t("empty")}</p>
        ) : (
          <div className="space-y-1">
            {revisions.map((rev) => (
              <div
                key={rev.id}
                className={cn(
                  "flex items-center justify-between rounded px-2 py-1.5 text-[11px]",
                  rev.revision === project.revision
                    ? "bg-brand-surface text-brand-foreground-strong-strong"
                    : diffCompareBase === rev.revision
                      ? "bg-info-surface text-info-foreground"
                      : "text-foreground hover:bg-surface-raised dark:text-muted-foreground dark:hover:bg-surface-base/50",
                )}
              >
                <div className="flex items-center gap-2">
                  <span className="font-mono font-medium">
                    {t("revisionLabel", { revision: rev.revision })}
                  </span>
                  <span className="text-muted-foreground">
                    {new Date(rev.created_at).toLocaleString(undefined, {
                      month: "short",
                      day: "numeric",
                      year: "numeric",
                      hour: "numeric",
                      minute: "2-digit",
                    })}
                  </span>
                  <span className="text-muted-foreground">
                    {t("nodeEdgeCount", { nodeCount: rev.node_count, edgeCount: rev.edge_count })}
                  </span>
                </div>
                <div className="flex items-center gap-1">
                  {/* Compare button: start compare mode or select target */}
                  {diffCompareBase !== null && diffCompareBase !== rev.revision && !diffLoading && (
                    <button
                      onClick={() => handleCompare(rev.revision)}
                      className="rounded px-1.5 py-0.5 text-2xs font-medium text-concept-foreground hover:bg-concept-surface dark:text-concept-foreground"
                    >
                      {t("compare")}
                    </button>
                  )}
                  {diffCompareBase === null && revisions.length > 1 && (
                    <button
                      onClick={() => setDiffCompareBase(rev.revision)}
                      className="rounded px-1.5 py-0.5 text-2xs font-medium text-concept-foreground hover:bg-concept-surface dark:text-concept-foreground"
                    >
                      {t("diff")}
                    </button>
                  )}
                  {rev.revision !== project.revision && !loading && (
                    <button
                      onClick={() => handleRestore(rev.revision)}
                      className="rounded px-1.5 py-0.5 text-2xs font-medium text-info-foreground hover:bg-info-surface dark:text-info-foreground"
                    >
                      {t("restore")}
                    </button>
                  )}
                  {rev.revision !== project.revision && !loading && (
                    <button
                      onClick={() => handleMigrate(rev.revision)}
                      className="rounded px-1.5 py-0.5 text-2xs font-medium text-warning-foreground hover:bg-warning-surface dark:hover:bg-warning-surface/30"
                    >
                      {t("migrate")}
                    </button>
                  )}
                  {rev.revision === project.revision && (
                    <span className="text-2xs font-medium text-brand-foreground">
                      {t("current")}
                    </span>
                  )}
                </div>
              </div>
            ))}
          </div>
        )}

        {/* Diff result panel */}
        {diffResult && (
          <div className="mt-3">
            <DiffPanel
              diff={diffResult}
              baseLabel={diffLabels.base}
              targetLabel={diffLabels.target}
              onDismiss={dismissDiff}
            />
          </div>
        )}

        {/* Migration result panel */}
        {migrationResult && migrationResult.up.length > 0 && (
          <div className="mt-3 space-y-2 rounded-lg border border-warning-border bg-warning-surface p-3">
            <h4 className="text-xs font-semibold text-warning-foreground">
              {t("migrationPreview")}
            </h4>
            {migrationResult.breaking_changes.length > 0 && (
              <div className="space-y-1">
                <p className="text-2xs font-semibold text-danger-foreground">{t("breakingChanges")}</p>
                {migrationResult.breaking_changes.map((bc, i) => (
                  <p key={i} className="text-2xs text-danger-foreground">{bc}</p>
                ))}
              </div>
            )}
            {migrationResult.warnings.length > 0 && (
              <div className="space-y-1">
                <p className="text-2xs font-semibold text-warning-foreground">{t("warnings")}</p>
                {migrationResult.warnings.map((w, i) => (
                  <p key={i} className="text-2xs text-warning-foreground">{w}</p>
                ))}
              </div>
            )}
            <pre className="max-h-32 overflow-auto rounded bg-surface-base p-2 text-2xs text-foreground-muted">
              {migrationResult.up.join(";\n")}
            </pre>
            <div className="flex gap-2">
              {migrationResult.breaking_changes.length === 0 && migrationTargetRev !== null && (
                <button
                  onClick={() => handleExecuteMigration(migrationTargetRev)}
                  disabled={loading}
                  className="rounded bg-warning-foreground px-3 py-1 text-2xs font-medium text-white hover:bg-warning-foreground disabled:opacity-50"
                >
                  {t("executeMigration")}
                </button>
              )}
              <button
                onClick={() => setMigrationResult(null)}
                className="rounded px-3 py-1 text-2xs font-medium text-foreground hover:bg-surface-inset dark:text-muted-foreground dark:hover:bg-surface-base"
              >
                {t("dismiss")}
              </button>
            </div>
          </div>
        )}
      </div>
    </details>
  );
}
