"use client";

import { cn } from "@/lib/cn";
import type { ReconcileReport } from "@/types/api";

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
  const hasUncertain = report.uncertain_matches.length > 0;
  const hasDeleted = report.deleted_entities.length > 0;
  const hasGenerated = report.generated_ids.length > 0;

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
          Refine Report
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
            {report.confidence}
          </span>
        </h4>
        <button
          onClick={onDismiss}
          className="text-zinc-400 hover:text-zinc-600"
        >
          ✕
        </button>
      </div>

      <div className="mt-2 space-y-1.5">
        {report.preserved_ids.length > 0 && (
          <p className="text-zinc-500">
            <span className="font-medium text-emerald-600">✓ Preserved:</span>{" "}
            {report.preserved_ids.length} entities
          </p>
        )}

        {hasGenerated && (
          <div>
            <p className="font-medium text-blue-600 dark:text-blue-400">
              + New ({report.generated_ids.length}):
            </p>
            <ul className="ml-3 mt-0.5 space-y-0.5">
              {report.generated_ids.map((e) => (
                <li key={e.id} className="text-zinc-600 dark:text-zinc-400">
                  {e.entity_kind}: {e.label}
                </li>
              ))}
            </ul>
          </div>
        )}

        {hasDeleted && (
          <div>
            <p className="font-medium text-red-600 dark:text-red-400">
              − Removed ({report.deleted_entities.length}):
            </p>
            <ul className="ml-3 mt-0.5 space-y-0.5">
              {report.deleted_entities.map((e) => (
                <li key={e.id} className="text-zinc-600 dark:text-zinc-400">
                  {e.entity_kind}: {e.label}
                </li>
              ))}
            </ul>
          </div>
        )}

        {hasUncertain && (
          <div>
            <p className="font-medium text-amber-600 dark:text-amber-400">
              ⚠ Uncertain matches ({report.uncertain_matches.length}):
            </p>
            <ul className="ml-3 mt-0.5 space-y-1">
              {report.uncertain_matches.map((m) => (
                <li key={m.original_id} className="text-zinc-600 dark:text-zinc-400">
                  <span className="font-medium">{m.original_label}</span>
                  {" → "}
                  <span className="font-medium">{m.matched_label}</span>
                  <span className="ml-1 text-zinc-400">({m.match_reason})</span>
                </li>
              ))}
            </ul>
          </div>
        )}
      </div>
    </div>
  );
}
