"use client";

import { useEffect, useState } from "react";
import { useTranslations } from "next-intl";
import { useAppStore } from "@/lib/store";
import { applyReconcile } from "@/lib/api";
import { cn } from "@/lib/cn";
import { toast } from "@/components/ui/toast";
import type {
  ReconcileReport,
  MatchDecision,
} from "@/types/api";

const CONFIDENCE_LEVELS = ["low", "medium", "high"] as const;
type KnownConfidence = (typeof CONFIDENCE_LEVELS)[number];
function isKnownConfidence(s: string): s is KnownConfidence {
  return (CONFIDENCE_LEVELS as readonly string[]).includes(s);
}

// ---------------------------------------------------------------------------
// Diff overlay dismiss bar -- shown when reconcile report is active
// ---------------------------------------------------------------------------

export function DiffOverlayBar() {
  const t = useTranslations("workbench.canvas.diffOverlay");
  const report = useAppStore((s) => s.lastReconcileReport);
  const setReport = useAppStore((s) => s.setLastReconcileReport);
  const pending = useAppStore((s) => s.pendingReconcile);
  const setPending = useAppStore((s) => s.setPendingReconcile);
  const activeProject = useAppStore((s) => s.activeProject);
  const applyProjectSnapshot = useAppStore((s) => s.applyProjectSnapshot);

  const [decisions, setDecisions] = useState<Record<string, boolean>>({});
  const [applying, setApplying] = useState(false);
  const [expanded, setExpanded] = useState(false);

  // Initialize decisions when pending reconcile changes (default: all accepted)
  useEffect(() => {
    if (pending) {
      const initial: Record<string, boolean> = {};
      for (const m of pending.report.uncertain_matches) {
        initial[m.original_id] = true; // default: accept
      }
      setDecisions(initial);
      setExpanded(true);
    } else {
      setDecisions({});
      setExpanded(false);
    }
  }, [pending]);

  if (!report) return null;

  const addedCount = report.generated_ids.length;
  const uncertainCount = report.uncertain_matches.length;
  const deletedCount = report.deleted_entities.length;

  const handleDismiss = () => {
    setReport(null);
    setPending(null);
  };

  const toggleDecision = (originalId: string) => {
    setDecisions((prev) => ({ ...prev, [originalId]: !prev[originalId] }));
  };

  const handleApplyDecisions = async () => {
    if (!pending || !activeProject) return;
    setApplying(true);
    try {
      const matchDecisions: MatchDecision[] =
        pending.report.uncertain_matches.map((m) => ({
          original_id: m.original_id,
          accept: decisions[m.original_id] ?? true,
        }));
      const resp = await applyReconcile(activeProject.id, {
        revision: activeProject.revision,
        reconciled_ontology: pending.reconciled_ontology,
        decisions: matchDecisions,
        uncertain_matches: pending.report.uncertain_matches,
      });
      applyProjectSnapshot(resp.project);
      setReport(resp.reconcile_report);
      setPending(null);
      toast.success(t("applySuccess"));
    } catch (err) {
      toast.error(t("applyFailed"), {
        description: err instanceof Error ? err.message : t("toast.unknownError"),
      });
    } finally {
      setApplying(false);
    }
  };

  return (
    <div className="absolute start-1/2 top-3 z-canvas -translate-x-1/2">
      <div
        className={cn(
          "rounded-lg border shadow-3 backdrop-blur-sm",
          report.confidence === "low"
            ? "border-danger-border bg-danger-surface"
            : report.confidence === "medium"
              ? "border-warning-border bg-warning-surface"
              : "border-brand-border bg-brand-surface",
        )}
      >
        {/* Summary row */}
        <div className="flex items-center gap-3 px-4 py-2 text-xs">
          <span className="font-semibold text-foreground">
            {t("title")}
          </span>
          <ConfidenceBadge confidence={report.confidence} />
          {addedCount > 0 && (
            <span className="text-brand-foreground">
              {t("addedCount", { count: addedCount })}
            </span>
          )}
          {uncertainCount > 0 && (
            <button type="button"
              onClick={() => pending && setExpanded((v) => !v)}
              className={cn(
                "text-warning-foreground",
                pending && "cursor-pointer underline decoration-dotted",
              )}
            >
              {t("uncertainCount", { count: uncertainCount })}
            </button>
          )}
          {deletedCount > 0 && (
            <span className="text-danger-foreground">
              {t("deletedCount", { count: deletedCount })}
            </span>
          )}
          <span className="text-foreground-muted">
            {t("preservedCount", { count: report.preserved_ids.length })}
          </span>
          {pending && (
            <button type="button"
              onClick={handleApplyDecisions}
              disabled={applying}
              className={cn(
                "ms-1 rounded-md bg-brand-solid px-3 py-1 text-foreground-onbrand hover:bg-brand-solid disabled:opacity-50",
                applying && "cursor-wait",
              )}
            >
              {applying ? t("applying") : t("applyDecisions")}
            </button>
          )}
          <button type="button"
            onClick={handleDismiss}
            className="ms-1 rounded-md px-2 py-0.5 text-foreground-muted hover:bg-surface-base hover:text-foreground"
          >
            {t("dismiss")}
          </button>
        </div>

        {/* Expanded uncertain match list */}
        {pending && expanded && uncertainCount > 0 && (
          <div className="border-t border-divider px-4 py-2">
            <div className="max-h-48 space-y-1.5 overflow-y-auto">
              {pending.report.uncertain_matches.map((m) => (
                <div
                  key={m.original_id}
                  className="flex items-center gap-2 rounded px-2 py-1 text-xs hover:bg-surface-base"
                >
                  <span className="min-w-0 flex-1 truncate text-foreground">
                    <span className="font-medium">{m.original_label}</span>
                    {m.original_label !== m.matched_label && (
                      <span className="text-foreground-muted">
                        {" -> "}
                        {m.matched_label}
                      </span>
                    )}
                    <span className="ms-1 text-foreground-muted">
                      ({m.entity_kind})
                    </span>
                    <span className="ms-1 italic text-foreground-muted">
                      {m.match_reason}
                    </span>
                  </span>
                  <button type="button"
                    onClick={() => toggleDecision(m.original_id)}
                    className={cn(
                      "shrink-0 rounded px-2 py-0.5 text-2xs font-semibold uppercase transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)]",
                      decisions[m.original_id]
                        ? "bg-brand-surface-strong text-brand-foreground hover:bg-brand-surface-strong"
                        : "bg-danger-surface text-danger-foreground hover:bg-danger-surface",
                    )}
                  >
                    {decisions[m.original_id] ? t("accept") : t("reject")}
                  </button>
                </div>
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

function ConfidenceBadge({
  confidence,
}: {
  confidence: ReconcileReport["confidence"];
}) {
  const t = useTranslations("workbench.canvas.diffOverlay");
  const label = isKnownConfidence(confidence)
    ? confidence === "high"
      ? t("confidenceHigh")
      : confidence === "medium"
        ? t("confidenceMedium")
        : t("confidenceLow")
    : confidence;
  return (
    <span
      className={cn(
        "rounded px-1.5 py-0.5 text-2xs font-bold uppercase",
        confidence === "high"
          ? "bg-brand-surface-strong text-brand-foreground-strong"
          : confidence === "medium"
            ? "bg-warning-surface text-warning-foreground"
            : "bg-danger-surface text-danger-foreground",
      )}
    >
      {label}
    </span>
  );
}
