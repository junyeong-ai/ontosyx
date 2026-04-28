"use client";

import { useState } from "react";
import { useTranslations } from "next-intl";
import { HugeiconsIcon } from "@hugeicons/react";
import { MagicWand01Icon, Refresh01Icon } from "@hugeicons/core-free-icons";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { Spinner } from "@/components/ui/spinner";
import { useAppStore } from "@/lib/store";
import { ApiError, adoptGraph, auditGraph } from "@/lib/api";
import type { GraphAuditReport } from "@/lib/api";
import { arr } from "@/lib/ir-collections";

export function GraphAuditSection({ ontologyId }: { ontologyId: string }) {
  const t = useTranslations("workbench.bottomPanel.workflowActions");
  const [report, setReport] = useState<GraphAuditReport | null>(null);
  const [loading, setLoading] = useState(false);
  const [adopting, setAdopting] = useState(false);
  const { loadStandaloneOntology } = useAppStore();

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
      loadStandaloneOntology(adopted);
      toast.success(
        t("adopted", {
          nodeCount: arr(adopted.node_types).length,
          edgeCount: arr(adopted.edge_types).length,
        }),
      );
      const result = await auditGraph(ontologyId);
      setReport(result);
    } catch (e) {
      toast.error(e instanceof ApiError ? e.message : t("adoptFailed"));
    } finally {
      setAdopting(false);
    }
  };

  const syncColor =
    report?.sync_status === "synced"
      ? "emerald"
      : report?.sync_status === "partial"
        ? "amber"
        : "red";

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
            {loading ? (
              <Spinner size="xs" />
            ) : (
              <HugeiconsIcon icon={Refresh01Icon} className="mr-1 h-3 w-3" />
            )}
            {t("auditGraph")}
          </Button>
        </div>
      ) : (
        <div className="space-y-2">
          <div className="flex items-center gap-2">
            <span
              className={`rounded-full px-2 py-0.5 text-[10px] font-medium ${
                syncColor === "emerald"
                  ? "bg-emerald-100 text-emerald-700 dark:bg-emerald-900 dark:text-emerald-300"
                  : syncColor === "amber"
                    ? "bg-amber-100 text-amber-700 dark:bg-amber-900 dark:text-amber-300"
                    : "bg-red-100 text-red-700 dark:bg-red-900 dark:text-red-300"
              }`}
            >
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

          {report.matched_nodes.length > 0 && (
            <p className="text-[10px] text-emerald-600 dark:text-emerald-400">
              {t("syncMatched", {
                nodeCount: report.matched_nodes.length,
                edgeCount: report.matched_edges.length,
              })}
            </p>
          )}

          {report.orphan_graph_edges.length > 0 && (
            <details className="text-[10px]">
              <summary className="cursor-pointer text-amber-600 dark:text-amber-400">
                {t("orphanGraphEdges", { count: report.orphan_graph_edges.length })}
              </summary>
              <div className="mt-1 flex flex-wrap gap-1">
                {report.orphan_graph_edges.map((e) => (
                  <span
                    key={e}
                    className="rounded bg-amber-100 px-1.5 py-0.5 text-amber-700 dark:bg-amber-900 dark:text-amber-300"
                  >
                    {e}
                  </span>
                ))}
              </div>
            </details>
          )}

          {report.missing_graph_edges.length > 0 && (
            <details className="text-[10px]">
              <summary className="cursor-pointer text-red-600 dark:text-red-400">
                {t("missingGraphEdges", { count: report.missing_graph_edges.length })}
              </summary>
              <div className="mt-1 flex flex-wrap gap-1">
                {report.missing_graph_edges.map((e) => (
                  <span
                    key={e}
                    className="rounded bg-red-100 px-1.5 py-0.5 text-red-700 dark:bg-red-900 dark:text-red-300"
                  >
                    {e}
                  </span>
                ))}
              </div>
            </details>
          )}

          <div className="flex gap-2">
            <Button size="sm" variant="ghost" onClick={handleAudit} disabled={loading}>
              {loading ? <Spinner size="xs" /> : t("reaudit")}
            </Button>
            {report.sync_status !== "synced" && (
              <Button size="sm" onClick={handleAdopt} disabled={adopting}>
                {adopting ? (
                  <Spinner size="xs" />
                ) : (
                  <HugeiconsIcon icon={MagicWand01Icon} className="mr-1 h-3 w-3" />
                )}
                {t("adoptGraphLabels")}
              </Button>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
