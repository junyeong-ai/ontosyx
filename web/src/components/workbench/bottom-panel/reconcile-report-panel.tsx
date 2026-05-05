"use client";

import { useTranslations } from "next-intl";
import { cn } from "@/lib/cn";
import type { ReconcileReport } from "@/types/api";

import { Heading } from "@/components/ui/heading";
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
          ? "border-danger-border bg-danger-surface"
          : report.confidence === "medium"
            ? "border-warning-border bg-warning-surface"
            : "border-brand-border bg-brand-surface",
      )}
    >
      <div className="flex items-center justify-between">
        <Heading level={4} size={6}>
          {t("title")}
          <span
            className={cn(
              "ms-2 rounded px-1.5 py-0.5 text-2xs font-bold uppercase",
              report.confidence === "low"
                ? "bg-danger-surface text-danger-foreground"
                : report.confidence === "medium"
                  ? "bg-warning-surface text-warning-foreground"
                  : "bg-brand-surface-strong text-brand-foreground-strong",
            )}
          >
            {confidenceLabel}
          </span>
        </Heading>
        <button type="button"
          onClick={onDismiss}
          className="text-foreground-muted hover:text-foreground"
        >
          ✕
        </button>
      </div>

      <div className="mt-2 space-y-1.5">
        {report.preserved_ids.length > 0 && (
          <p className="text-foreground-muted">
            <span className="font-medium text-brand-foreground">{t("preserved")}</span>{" "}
            {t("preservedCount", { count: report.preserved_ids.length })}
          </p>
        )}

        {hasGenerated && (
          <div>
            <p className="font-medium text-info-foreground">
              {t("added", { count: report.generated_ids.length })}
            </p>
            <ul className="ms-3 mt-0.5 space-y-0.5">
              {report.generated_ids.map((e) => (
                <li key={e.id} className="text-foreground">
                  {e.entity_kind}: {e.label}
                </li>
              ))}
            </ul>
          </div>
        )}

        {hasDeleted && (
          <div>
            <p className="font-medium text-danger-foreground">
              {t("removed", { count: report.deleted_entities.length })}
            </p>
            <ul className="ms-3 mt-0.5 space-y-0.5">
              {report.deleted_entities.map((e) => (
                <li key={e.id} className="text-foreground">
                  {e.entity_kind}: {e.label}
                </li>
              ))}
            </ul>
          </div>
        )}

        {hasUncertain && (
          <div>
            <p className="font-medium text-warning-foreground">
              {t("uncertain", { count: report.uncertain_matches.length })}
            </p>
            <ul className="ms-3 mt-0.5 space-y-1">
              {report.uncertain_matches.map((m) => (
                <li key={m.original_id} className="text-foreground">
                  <span className="font-medium">{m.original_label}</span>
                  {" → "}
                  <span className="font-medium">{m.matched_label}</span>
                  <span className="ms-1 text-foreground-muted">({m.match_reason})</span>
                </li>
              ))}
            </ul>
          </div>
        )}
      </div>
    </div>
  );
}
