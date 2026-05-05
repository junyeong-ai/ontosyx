"use client";

import { useState } from "react";
import { useTranslations } from "next-intl";
import { RefreshCw, Wand2 } from "lucide-react";
import { toast } from "@/components/ui/toast";

import { Button } from "@/components/ui/button";
import { Eyebrow } from "@/components/ui/eyebrow";
import { Spinner } from "@/components/ui/spinner";
import { useAppStore } from "@/lib/store";
import { ApiError, adoptGraph, auditGraph } from "@/lib/api";
import type { GraphAuditReport } from "@/lib/api";
import { arr } from "@/lib/ir-collections";

export function GraphAuditSection() {
  const t = useTranslations("workbench.bottomPanel.workflowActions");
  const [report, setReport] = useState<GraphAuditReport | null>(null);
  const [loading, setLoading] = useState(false);
  const [adopting, setAdopting] = useState(false);
  const { loadStandaloneOntology } = useAppStore();

  const handleAudit = async () => {
    setLoading(true);
    try {
      const result = await auditGraph();
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
      const result = await auditGraph();
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
    <div className="space-y-2 rounded-lg border border-success-border bg-success-surface/50 p-3">
      <Eyebrow level={4} size="dense" tone="success" caps="none">
        {t("graphSync")}
      </Eyebrow>

      {!report ? (
        <div className="space-y-2">
          <p className="text-2xs text-success-foreground">
            {t("graphSyncDescription")}
          </p>
          <Button size="sm" onClick={handleAudit} disabled={loading}>
            {loading ? (
              <Spinner size="xs" />
            ) : (
              <RefreshCw className="me-1 h-3 w-3" />
            )}
            {t("auditGraph")}
          </Button>
        </div>
      ) : (
        <div className="space-y-2">
          <div className="flex items-center gap-2">
            <span
              className={`rounded-full px-2 py-0.5 text-2xs font-medium ${
                syncColor === "emerald"
                  ? "bg-brand-surface-strong text-brand-foreground-strong"
                  : syncColor === "amber"
                    ? "bg-warning-surface text-warning-foreground"
                    : "bg-danger-surface text-danger-foreground"
              }`}
            >
              {report.sync_status === "synced"
                ? t("syncStatusSynced")
                : report.sync_status === "partial"
                  ? t("syncStatusPartial")
                  : t("syncStatusUnsynced")}
            </span>
            <span className="text-2xs text-foreground-muted">
              {t("syncPercentage", { percent: report.sync_percentage })}
            </span>
          </div>

          {report.matched_nodes.length > 0 && (
            <p className="text-2xs text-brand-foreground">
              {t("syncMatched", {
                nodeCount: report.matched_nodes.length,
                edgeCount: report.matched_edges.length,
              })}
            </p>
          )}

          {report.orphan_graph_edges.length > 0 && (
            <details className="text-2xs">
              <summary className="cursor-pointer text-warning-foreground">
                {t("orphanGraphEdges", { count: report.orphan_graph_edges.length })}
              </summary>
              <div className="mt-1 flex flex-wrap gap-1">
                {report.orphan_graph_edges.map((e) => (
                  <span
                    key={e}
                    className="rounded bg-warning-surface px-1.5 py-0.5 text-warning-foreground"
                  >
                    {e}
                  </span>
                ))}
              </div>
            </details>
          )}

          {report.missing_graph_edges.length > 0 && (
            <details className="text-2xs">
              <summary className="cursor-pointer text-danger-foreground">
                {t("missingGraphEdges", { count: report.missing_graph_edges.length })}
              </summary>
              <div className="mt-1 flex flex-wrap gap-1">
                {report.missing_graph_edges.map((e) => (
                  <span
                    key={e}
                    className="rounded bg-danger-surface px-1.5 py-0.5 text-danger-foreground"
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
                  <Wand2 className="me-1 h-3 w-3" />
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
