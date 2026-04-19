"use client";

import { useTranslations } from "next-intl";
import { cn } from "@/lib/cn";
import type { ReconcileReport } from "@/types/api";

const CONFIDENCE_LEVELS = ["low", "medium", "high"] as const;
type KnownConfidence = (typeof CONFIDENCE_LEVELS)[number];
function isKnownConfidence(s: string): s is KnownConfidence {
  return (CONFIDENCE_LEVELS as readonly string[]).includes(s);
}

// ---------------------------------------------------------------------------
// Reconcile Report — shown after LLM refine
// ---------------------------------------------------------------------------

export function ReconcileReportPanel({
  report,
  onDismiss,
}: {
  report: ReconcileReport;
  onDismiss: () => void;
}) {
  const t = useTranslations("workbench.bottomPanel.reconcile");
  const hasUncertain = report.uncertain_matches.length > 0;
  const hasDeleted = report.deleted_entities.length > 0;
  const hasGenerated = report.generated_ids.length > 0;
  const confidenceLabel = isKnownConfidence(report.confidence)
    ? t(`confidence${report.confidence.charAt(0).toUpperCase()}${report.confidence.slice(1)}` as
        | "confidenceLow"
        | "confidenceMedium"
        | "confidenceHigh")
    : report.confidence;

  return (
    <div
      className={cn(
        "mt-2 rounded-lg border p-3 text-xs",
        report.confidence === "low"
          ? "border-red-200 bg-red-50/50 dark:border-red-900 dark:bg-red-950/20"
          : report.confidence === "medium"
            ? "border-amber-200 bg-amber-50/50 dark:border-amber-900 dark:bg-amber-950/20"
            : "border-emerald-200 bg-emerald-50/50 dark:border-emerald-900 dark:bg-emerald-950/20",
      )}
    >
      <div className="flex items-center justify-between">
        <h4 className="font-semibold text-zinc-700 dark:text-zinc-300">
          {t("title")}
          <span
            className={cn(
              "ml-2 rounded px-1.5 py-0.5 text-[9px] font-bold uppercase",
              report.confidence === "low"
                ? "bg-red-100 text-red-700 dark:bg-red-900 dark:text-red-300"
                : report.confidence === "medium"
                  ? "bg-amber-100 text-amber-700 dark:bg-amber-900 dark:text-amber-300"
                  : "bg-emerald-100 text-emerald-700 dark:bg-emerald-900 dark:text-emerald-300",
            )}
          >
            {confidenceLabel}
          </span>
        </h4>
        <button
          onClick={onDismiss}
          className="text-muted-foreground hover:text-zinc-600"
        >
          ✕
        </button>
      </div>

      <div className="mt-2 space-y-1.5">
        {report.preserved_ids.length > 0 && (
          <p className="text-muted-foreground">
            <span className="font-medium text-emerald-600">{t("preserved")}</span>{" "}
            {t("preservedCount", { count: report.preserved_ids.length })}
          </p>
        )}

        {hasGenerated && (
          <div>
            <p className="font-medium text-blue-600 dark:text-blue-400">
              {t("added", { count: report.generated_ids.length })}
            </p>
            <ul className="ml-3 mt-0.5 space-y-0.5">
              {report.generated_ids.map((e) => (
                <li key={e.id} className="text-zinc-600 dark:text-muted-foreground">
                  {e.entity_kind}: {e.label}
                </li>
              ))}
            </ul>
          </div>
        )}

        {hasDeleted && (
          <div>
            <p className="font-medium text-red-600 dark:text-red-400">
              {t("removed", { count: report.deleted_entities.length })}
            </p>
            <ul className="ml-3 mt-0.5 space-y-0.5">
              {report.deleted_entities.map((e) => (
                <li key={e.id} className="text-zinc-600 dark:text-muted-foreground">
                  {e.entity_kind}: {e.label}
                </li>
              ))}
            </ul>
          </div>
        )}

        {hasUncertain && (
          <div>
            <p className="font-medium text-amber-600 dark:text-amber-400">
              {t("uncertain", { count: report.uncertain_matches.length })}
            </p>
            <ul className="ml-3 mt-0.5 space-y-1">
              {report.uncertain_matches.map((m) => (
                <li key={m.original_id} className="text-zinc-600 dark:text-muted-foreground">
                  <span className="font-medium">{m.original_label}</span>
                  {" → "}
                  <span className="font-medium">{m.matched_label}</span>
                  <span className="ml-1 text-muted-foreground">({m.match_reason})</span>
                </li>
              ))}
            </ul>
          </div>
        )}
      </div>
    </div>
  );
}
