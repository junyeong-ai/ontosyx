"use client";

import { useCallback, useMemo } from "react";
import { useTranslations } from "next-intl";

import { Button } from "@/components/ui/button";
import { Heading } from "@/components/ui/heading";
import { cn } from "@/lib/cn";
import { toCsv, triggerCsvDownload } from "@/lib/csv";
import type { components } from "@/types/api.generated";

type RetrievalComparisonAggregate =
  components["schemas"]["RetrievalComparisonAggregate"];
type RetrievalSurface = components["schemas"]["RetrievalSurface"];

/**
 * Run-level hybrid-vs-trigram aggregate.
 *
 * Folds the per-case 8-metric `<surface>.<leg>.<axis>` rows into
 * a per-(surface, axis) matrix. Each cell shows the mean lift,
 * the win-rate (% of paired cases where hybrid beat trigram),
 * and the paired-case denominator so a 1-case cell isn't read
 * the same as a 30-case cell.
 *
 * Visual language matches the case-level
 * `RetrievalComparisonView`: success tone for `lift > 1e-6`,
 * danger for `< -1e-6`, muted for parity. The same 1e-6
 * tolerance suppresses f64 round-trip noise that an aggregate's
 * `AVG()` can produce even when every paired delta was zero.
 *
 * Renders nothing when the run has no `retrieval_comparison`
 * cases — the BE skip-if-empty serde gate keeps the field absent
 * in that case, so the FE switch reads as length-based.
 */
export interface RunComparisonAggregateProps {
  rows: readonly RetrievalComparisonAggregate[];
  /**
   * Run identifier — used in the CSV download filename so
   * exported sheets resolve to a specific run when the operator
   * shares them in Slack or Excel. Optional so the component
   * still renders correctly inside fixture-based unit tests.
   */
  runId?: string;
}

const SURFACE_ORDER: readonly RetrievalSurface[] = [
  "verified_query",
  "community_summary",
  "knowledge_entry",
];

const AXIS_ORDER = [
  "precision_at_k",
  "recall_at_k",
  "mrr",
  "ndcg_at_k",
] as const;

export function RunComparisonAggregate({
  rows,
  runId,
}: RunComparisonAggregateProps) {
  const t = useTranslations("settings.evaluation.detail.runComparisonAggregate");

  const grid = useMemo(() => {
    const map = new Map<string, RetrievalComparisonAggregate>();
    for (const row of rows) {
      map.set(`${row.surface}\x1f${row.axis}`, row);
    }
    return map;
  }, [rows]);

  const onDownloadCsv = useCallback(() => {
    const header = [
      "surface",
      "axis",
      "paired_case_count",
      "hybrid_mean",
      "trigram_mean",
      "mean_lift",
      "win_rate_pct",
    ];
    // Sort by (surface, axis) for stable downloaded output —
    // the BE already emits in that order, but we re-sort
    // defensively so an out-of-order props array doesn't shift
    // the CSV between runs.
    const sorted = [...rows].sort((a, b) => {
      if (a.surface !== b.surface) return a.surface < b.surface ? -1 : 1;
      return a.axis < b.axis ? -1 : a.axis > b.axis ? 1 : 0;
    });
    const body = sorted.map((r) => [
      r.surface,
      r.axis,
      r.paired_case_count,
      r.hybrid_mean.toFixed(6),
      r.trigram_mean.toFixed(6),
      r.mean_lift.toFixed(6),
      r.win_rate_pct.toFixed(2),
    ]);
    const csv = toCsv(header, body);
    const filename = runId
      ? `retrieval-comparison-${runId}.csv`
      : "retrieval-comparison.csv";
    triggerCsvDownload(filename, csv);
  }, [rows, runId]);

  if (rows.length === 0) return null;

  return (
    <section className="mb-6 rounded-xl border border-divider bg-surface-base p-4">
      <header className="flex items-baseline justify-between gap-3">
        <Heading level={2} size={5}>
          {t("title")}
        </Heading>
        <Button
          type="button"
          variant="outline"
          size="sm"
          onClick={onDownloadCsv}
        >
          {t("downloadCsv")}
        </Button>
      </header>
      <p className="mt-1 text-xs text-foreground-muted">{t("description")}</p>

      <div className="mt-3 overflow-x-auto">
        <table className="w-full min-w-[680px] border-collapse text-sm">
          <thead>
            <tr className="text-2xs font-medium uppercase tracking-wide text-foreground-muted">
              <th scope="col" className="py-2 pe-3 text-start">
                {t("surfaceHeader")}
              </th>
              {AXIS_ORDER.map((axis) => (
                <th
                  key={axis}
                  scope="col"
                  className="py-2 pe-3 text-end tabular-nums"
                >
                  {t(`axis.${axisI18nKey(axis)}`)}
                </th>
              ))}
            </tr>
          </thead>
          <tbody className="divide-y divide-divider">
            {SURFACE_ORDER.map((surface) => {
              const surfaceCells = AXIS_ORDER.map((axis) =>
                grid.get(`${surface}\x1f${axis}`),
              );
              if (surfaceCells.every((c) => c === undefined)) return null;
              return (
                <tr key={surface}>
                  <th
                    scope="row"
                    className="py-2 pe-3 text-start font-medium text-foreground-strong"
                  >
                    {t(`surface.${surfaceI18nKey(surface)}`)}
                  </th>
                  {AXIS_ORDER.map((axis, i) => (
                    <Cell key={axis} cell={surfaceCells[i]} />
                  ))}
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </section>
  );
}

function Cell({ cell }: { cell: RetrievalComparisonAggregate | undefined }) {
  const t = useTranslations("settings.evaluation.detail.runComparisonAggregate");
  if (cell === undefined) {
    return (
      <td
        className="py-2 pe-3 text-end text-2xs text-foreground-muted"
        aria-label={t("emptyCell")}
      >
        —
      </td>
    );
  }
  const lift = cell.mean_lift;
  const tone = lift > 1e-6 ? "win" : lift < -1e-6 ? "loss" : "parity";
  return (
    <td className="py-2 pe-3 text-end tabular-nums">
      <div
        className={cn(
          "text-sm font-medium",
          tone === "win" && "text-success-foreground",
          tone === "loss" && "text-danger-foreground",
          tone === "parity" && "text-foreground-strong",
        )}
        aria-label={t("liftAriaLabel", {
          lift: lift.toFixed(3),
          winRate: cell.win_rate_pct.toFixed(0),
          n: cell.paired_case_count,
        })}
      >
        {tone === "parity" ? "±0.000" : (lift > 0 ? "+" : "") + lift.toFixed(3)}
      </div>
      <div className="mt-0.5 text-2xs text-foreground-muted">
        {t("winRateNCases", {
          winRate: cell.win_rate_pct.toFixed(0),
          n: cell.paired_case_count,
        })}
      </div>
    </td>
  );
}

function axisI18nKey(axis: (typeof AXIS_ORDER)[number]): string {
  // Map snake_case wire keys to camelCase i18n keys without
  // dotted segments (next-intl path collision guard).
  switch (axis) {
    case "precision_at_k":
      return "precisionAtK";
    case "recall_at_k":
      return "recallAtK";
    case "mrr":
      return "mrr";
    case "ndcg_at_k":
      return "ndcgAtK";
  }
}

function surfaceI18nKey(surface: RetrievalSurface): string {
  switch (surface) {
    case "verified_query":
      return "verifiedQuery";
    case "community_summary":
      return "communitySummary";
    case "knowledge_entry":
      return "knowledgeEntry";
  }
}
