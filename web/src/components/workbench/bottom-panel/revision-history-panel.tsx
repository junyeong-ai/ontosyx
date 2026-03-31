"use client";

import { useState, useEffect } from "react";
import { Spinner } from "@/components/ui/spinner";
import { useAppStore } from "@/lib/store";
import { useConfirm } from "@/components/ui/confirm-dialog";
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
  OntologyIR,
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
  setProject: (p: DesignProject | null) => void;
  setOntology: (o: OntologyIR) => void;
  onApiError: (err: unknown, label: string) => Promise<boolean>;
}

// ---------------------------------------------------------------------------
// Revision History Panel
// ---------------------------------------------------------------------------

export function RevisionHistoryPanel({
  project,
  loading,
  setLoading,
  setProject,
  setOntology,
  onApiError,
}: RevisionHistoryPanelProps) {
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
        base: `Rev ${diffCompareBase}`,
        target: `Rev ${targetRevision}`,
      });
      setDiffCompareBase(null);
      setActiveDiffOverlay(diff);
    } catch (e) {
      const msg = e instanceof ApiError ? e.message : "Failed to compute diff";
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
      title: "Restore Revision",
      description: `Restore ontology to revision ${rev}? The current ontology will be saved as a snapshot before restoring.`,
      confirmLabel: "Restore",
      variant: "warning",
    });
    if (!ok) return;

    setLoading(true);
    try {
      const resp = await restoreRevision(project.id, rev);
      setProject(resp.project);
      if (resp.project.ontology) {
        setOntology(resp.project.ontology as OntologyIR);
      }
      loadRevisions();
      toast.success(`Restored to revision ${rev}`);
    } catch (err) {
      if (await onApiError(err, "Restore failed")) return;
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
        toast.info("No schema changes between revisions");
      }
    } catch (err) {
      if (await onApiError(err, "Migration preview failed")) return;
    } finally {
      setLoading(false);
    }
  }

  async function handleExecuteMigration(rev: number) {
    setLoading(true);
    try {
      const resp = await migrateSchema(project.id, rev, { dry_run: false });
      setMigrationResult(null);
      toast.success(`Migration executed: ${resp.up.length} statements`);
    } catch (err) {
      if (await onApiError(err, "Migration failed")) return;
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
      <summary className="cursor-pointer text-xs font-semibold uppercase tracking-wider text-zinc-500 hover:text-zinc-700 dark:hover:text-zinc-300">
        Revision History
      </summary>
      <div className="mt-2">
        {/* Compare mode instructions */}
        {diffCompareBase !== null && (
          <div className="mb-2 flex items-center gap-2 rounded bg-blue-50 px-2 py-1.5 text-[11px] text-blue-700 dark:bg-blue-950/30 dark:text-blue-300">
            <span>Base: Rev {diffCompareBase}. Click another revision to compare.</span>
            <button
              onClick={() => setDiffCompareBase(null)}
              className="ml-auto text-[10px] font-medium text-blue-500 hover:text-blue-700 dark:text-blue-400"
            >
              Cancel
            </button>
          </div>
        )}
        {diffLoading && (
          <div className="mb-2 flex items-center gap-2 py-1 text-xs text-zinc-500">
            <Spinner size="xs" /> Computing diff...
          </div>
        )}
        {revisionsLoading ? (
          <div className="flex items-center gap-2 py-2 text-xs text-zinc-500">
            <Spinner size="xs" /> Loading revisions...
          </div>
        ) : revisions.length === 0 ? (
          <p className="py-2 text-xs text-zinc-400">No revision history yet.</p>
        ) : (
          <div className="space-y-1">
            {revisions.map((rev) => (
              <div
                key={rev.id}
                className={cn(
                  "flex items-center justify-between rounded px-2 py-1.5 text-[11px]",
                  rev.revision === project.revision
                    ? "bg-emerald-50 text-emerald-800 dark:bg-emerald-950/30 dark:text-emerald-200"
                    : diffCompareBase === rev.revision
                      ? "bg-blue-50 text-blue-800 dark:bg-blue-950/30 dark:text-blue-200"
                      : "text-zinc-600 hover:bg-zinc-50 dark:text-zinc-400 dark:hover:bg-zinc-800/50",
                )}
              >
                <div className="flex items-center gap-2">
                  <span className="font-mono font-medium">
                    Rev {rev.revision}
                  </span>
                  <span className="text-zinc-400 dark:text-zinc-500">
                    {new Date(rev.created_at).toLocaleString(undefined, {
                      month: "short",
                      day: "numeric",
                      year: "numeric",
                      hour: "numeric",
                      minute: "2-digit",
                    })}
                  </span>
                  <span className="text-zinc-400 dark:text-zinc-500">
                    {rev.node_count}N, {rev.edge_count}E
                  </span>
                </div>
                <div className="flex items-center gap-1">
                  {/* Compare button: start compare mode or select target */}
                  {diffCompareBase !== null && diffCompareBase !== rev.revision && !diffLoading && (
                    <button
                      onClick={() => handleCompare(rev.revision)}
                      className="rounded px-1.5 py-0.5 text-[10px] font-medium text-purple-600 hover:bg-purple-50 dark:text-purple-400 dark:hover:bg-purple-950/30"
                    >
                      Compare
                    </button>
                  )}
                  {diffCompareBase === null && revisions.length > 1 && (
                    <button
                      onClick={() => setDiffCompareBase(rev.revision)}
                      className="rounded px-1.5 py-0.5 text-[10px] font-medium text-purple-600 hover:bg-purple-50 dark:text-purple-400 dark:hover:bg-purple-950/30"
                    >
                      Diff
                    </button>
                  )}
                  {rev.revision !== project.revision && !loading && (
                    <button
                      onClick={() => handleRestore(rev.revision)}
                      className="rounded px-1.5 py-0.5 text-[10px] font-medium text-blue-600 hover:bg-blue-50 dark:text-blue-400 dark:hover:bg-blue-950/30"
                    >
                      Restore
                    </button>
                  )}
                  {rev.revision !== project.revision && !loading && (
                    <button
                      onClick={() => handleMigrate(rev.revision)}
                      className="rounded px-1.5 py-0.5 text-[10px] font-medium text-amber-600 hover:bg-amber-50 dark:text-amber-400 dark:hover:bg-amber-950/30"
                    >
                      Migrate
                    </button>
                  )}
                  {rev.revision === project.revision && (
                    <span className="text-[10px] font-medium text-emerald-600 dark:text-emerald-400">
                      current
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
          <div className="mt-3 space-y-2 rounded-lg border border-amber-200 bg-amber-50/50 p-3 dark:border-amber-900 dark:bg-amber-950/20">
            <h4 className="text-xs font-semibold text-amber-800 dark:text-amber-200">
              Migration Preview
            </h4>
            {migrationResult.breaking_changes.length > 0 && (
              <div className="space-y-1">
                <p className="text-[10px] font-semibold text-red-600">Breaking Changes:</p>
                {migrationResult.breaking_changes.map((bc, i) => (
                  <p key={i} className="text-[10px] text-red-600">{bc}</p>
                ))}
              </div>
            )}
            {migrationResult.warnings.length > 0 && (
              <div className="space-y-1">
                <p className="text-[10px] font-semibold text-amber-600">Warnings:</p>
                {migrationResult.warnings.map((w, i) => (
                  <p key={i} className="text-[10px] text-amber-600">{w}</p>
                ))}
              </div>
            )}
            <pre className="max-h-32 overflow-auto rounded bg-zinc-900 p-2 text-[10px] text-zinc-300">
              {migrationResult.up.join(";\n")}
            </pre>
            <div className="flex gap-2">
              {migrationResult.breaking_changes.length === 0 && migrationTargetRev !== null && (
                <button
                  onClick={() => handleExecuteMigration(migrationTargetRev)}
                  disabled={loading}
                  className="rounded bg-amber-600 px-3 py-1 text-[10px] font-medium text-white hover:bg-amber-700 disabled:opacity-50"
                >
                  Execute Migration
                </button>
              )}
              <button
                onClick={() => setMigrationResult(null)}
                className="rounded px-3 py-1 text-[10px] font-medium text-zinc-600 hover:bg-zinc-100 dark:text-zinc-400 dark:hover:bg-zinc-800"
              >
                Dismiss
              </button>
            </div>
          </div>
        )}
      </div>
    </details>
  );
}
