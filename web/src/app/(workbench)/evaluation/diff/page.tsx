"use client";

import Link from "next/link";
import { useTranslations } from "next-intl";
import { useCallback, useMemo, useState } from "react";

import { useAuth } from "@/hooks/use-auth";
import {
  useEvaluationRunComparisonOutliers,
  useEvaluationRunDiff,
  useEvaluationRuns,
} from "@/hooks/api/use-evaluation";
import { WorkbenchPageShell } from "@/components/workbench/workbench-page-shell";
import { PageStateView } from "@/components/layout/page-state-view";
import type { PageState } from "@/components/layout/page-state";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { SkeletonList } from "@/components/ui/skeleton";
import { Heading } from "@/components/ui/heading";
import { SettingsSelect } from "@/components/ui/form-input";
import { cn } from "@/lib/cn";
import { toCsv, triggerCsvDownload } from "@/lib/csv";
import type { components } from "@/types/api.generated";
import type {
  EvaluationRun,
  RunAxisSummary,
  RunMetricDelta,
} from "@/types/evaluation";

type RetrievalComparisonDelta =
  components["schemas"]["RetrievalComparisonDelta"];
type RetrievalComparisonOutlier =
  components["schemas"]["RetrievalComparisonOutlier"];
type RetrievalLiftRegressionAlert =
  components["schemas"]["RetrievalLiftRegressionAlert"];

/** Map a `mean_delta` to a style hint — green improvement,
 *  red regression, neutral when the delta is below the
 *  noise floor (`< 0.005`). The same banding drives the per-
 *  case row colouring; centralised so the threshold only
 *  lives in one place. */
function deltaTone(
  delta: number,
): "improvement" | "regression" | "neutral" {
  if (delta > 0.005) return "improvement";
  if (delta < -0.005) return "regression";
  return "neutral";
}

const TONE_FG: Record<ReturnType<typeof deltaTone>, string> = {
  improvement: "text-success-foreground",
  regression: "text-danger-foreground",
  neutral: "text-foreground-muted",
};

const TONE_BG: Record<ReturnType<typeof deltaTone>, string> = {
  improvement: "bg-success-surface",
  regression: "bg-danger-surface",
  neutral: "bg-surface-inset",
};

/** Format a numeric delta with sign + 3 decimals. Aligns with
 *  the per-axis aggregate convention; positive = candidate
 *  improved. */
function formatDelta(value: number): string {
  const sign = value > 0 ? "+" : value < 0 ? "" : " ";
  return `${sign}${value.toFixed(3)}`;
}

/** Format Cohen's d with the canonical interpretation hint
 *  ("medium", "large", …). Matches the patent docstring on
 *  `RunAxisSummary.cohen_d`. */
function formatCohenD(t: ReturnType<typeof useTranslations>, d?: number): string {
  if (d === undefined) return "—";
  const abs = Math.abs(d);
  let band: "negligible" | "small" | "medium" | "large";
  if (abs < 0.2) band = "negligible";
  else if (abs < 0.5) band = "small";
  else if (abs < 0.8) band = "medium";
  else band = "large";
  return `${d.toFixed(2)} (${t(`bandLabel.${band}`)})`;
}

/// Auto-alarm banner for hybrid lift regressions detected in
/// the candidate run. Each alert is one (surface, axis) cell
/// whose `lift_delta` crossed the BE threshold AND landed on
/// enough paired cases to clear the noise floor. The threshold
/// is echoed onto each alert so the FE renders both the
/// observed delta and the cut without re-deriving the
/// constant.
///
/// Renders nothing when no regression is detected — the BE
/// skip-if-empty serde gate keeps the field absent in that
/// case, so the FE switch reads as length-based.
function RetrievalLiftRegressionBanner({
  alerts,
}: {
  alerts: readonly RetrievalLiftRegressionAlert[];
}) {
  const t = useTranslations("settings.evaluation.diff");
  if (alerts.length === 0) return null;
  return (
    <section
      role="alert"
      aria-labelledby="retrieval-lift-regression-heading"
      className="mb-6 rounded-xl border border-danger-border bg-danger-surface p-4"
    >
      <Heading
        level={2}
        size={5}
        className="text-danger-foreground"
        id="retrieval-lift-regression-heading"
      >
        {t("retrievalLiftRegression.title", { count: alerts.length })}
      </Heading>
      <p className="mt-1 text-xs text-danger-foreground">
        {t("retrievalLiftRegression.description", {
          threshold: alerts[0].threshold.toFixed(3),
        })}
      </p>
      <ul className="mt-3 space-y-1.5">
        {alerts.map((a) => (
          <li
            key={`${a.surface}\x1f${a.axis}`}
            className="grid grid-cols-[1fr_auto_auto] items-baseline gap-3 rounded-md bg-surface-base px-3 py-2"
          >
            <span className="font-medium text-foreground-strong">
              {t(
                `retrievalLiftDelta.surface.${
                  a.surface === "verified_query"
                    ? "verifiedQuery"
                    : a.surface === "community_summary"
                      ? "communitySummary"
                      : "knowledgeEntry"
                }`,
              )}
              <span className="ms-2 text-2xs text-foreground-muted">
                {a.axis}
              </span>
            </span>
            <span className="tabular-nums text-foreground-muted">
              {t("retrievalLiftRegression.observedDelta", {
                delta: a.lift_delta.toFixed(3),
              })}
            </span>
            <span className="tabular-nums text-2xs text-foreground-muted">
              {t("retrievalLiftRegression.pairedNCases", {
                n: a.candidate_paired_case_count,
              })}
            </span>
          </li>
        ))}
      </ul>
    </section>
  );
}

/// Render the run-vs-run hybrid retrieval lift delta. Each row
/// is one (surface, axis) cell carrying both runs' average lift
/// + the inter-run delta. Tone keys off `lift_delta` so a
/// regression (candidate's lift dropped vs baseline) reads in
/// danger tone, an improvement in success tone, parity in
/// muted. Section disappears when neither run had any
/// retrieval_comparison cases (BE skips the field).
function RetrievalLiftDeltaSection({
  rows,
  baselineRunId,
  candidateRunId,
}: {
  rows: readonly RetrievalComparisonDelta[];
  baselineRunId: string;
  candidateRunId: string;
}) {
  const t = useTranslations("settings.evaluation.diff");
  const [outliersOpen, setOutliersOpen] = useState(false);
  // Drill into the candidate run's outliers — that's the run
  // operators inspect for regressions. Fetched lazily so the
  // page doesn't pay the round-trip until the operator actually
  // wants the drill-down.
  const outliersQuery = useEvaluationRunComparisonOutliers(
    candidateRunId,
    { limit: 10 },
    outliersOpen,
  );
  const onDownloadCsv = useCallback(() => {
    const header = [
      "surface",
      "axis",
      "baseline_lift",
      "candidate_lift",
      "lift_delta",
      "baseline_paired",
      "candidate_paired",
    ];
    const sorted = [...rows].sort((a, b) => {
      if (a.surface !== b.surface) return a.surface < b.surface ? -1 : 1;
      return a.axis < b.axis ? -1 : a.axis > b.axis ? 1 : 0;
    });
    const body = sorted.map((r) => [
      r.surface,
      r.axis,
      r.baseline_lift.toFixed(6),
      r.candidate_lift.toFixed(6),
      r.lift_delta.toFixed(6),
      r.baseline_paired_case_count,
      r.candidate_paired_case_count,
    ]);
    const csv = toCsv(header, body);
    triggerCsvDownload(
      `retrieval-lift-diff-${baselineRunId}-${candidateRunId}.csv`,
      csv,
    );
  }, [rows, baselineRunId, candidateRunId]);

  if (rows.length === 0) return null;
  return (
    <section className="mb-6 rounded-xl border border-divider bg-surface-base p-4">
      <header className="flex items-baseline justify-between gap-3">
        <Heading level={2} size={5}>
          {t("retrievalLiftDelta.title")}
        </Heading>
        <div className="flex items-center gap-2">
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={() => setOutliersOpen((open) => !open)}
            aria-expanded={outliersOpen}
          >
            {outliersOpen
              ? t("retrievalLiftDelta.hideOutliers")
              : t("retrievalLiftDelta.showOutliers")}
          </Button>
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={onDownloadCsv}
          >
            {t("retrievalLiftDelta.downloadCsv")}
          </Button>
        </div>
      </header>
      <p className="mt-1 text-xs text-foreground-muted">
        {t("retrievalLiftDelta.description")}
      </p>
      <div className="mt-3 overflow-x-auto">
        <table className="w-full text-sm">
          <thead>
            <tr className="text-start text-2xs font-medium uppercase tracking-wide text-foreground-muted">
              <th className="px-4 py-2 text-start">
                {t("retrievalLiftDelta.col.surface")}
              </th>
              <th className="px-4 py-2 text-start">
                {t("retrievalLiftDelta.col.axis")}
              </th>
              <th className="px-4 py-2 text-end">
                {t("retrievalLiftDelta.col.baselineLift")}
              </th>
              <th className="px-4 py-2 text-end">
                {t("retrievalLiftDelta.col.candidateLift")}
              </th>
              <th className="px-4 py-2 text-end">
                {t("retrievalLiftDelta.col.liftDelta")}
              </th>
              <th className="px-4 py-2 text-end">
                {t("retrievalLiftDelta.col.pairedCounts")}
              </th>
            </tr>
          </thead>
          <tbody>
            {rows.map((row) => (
              <RetrievalLiftDeltaRow
                key={`${row.surface}\x1f${row.axis}`}
                row={row}
                t={t}
              />
            ))}
          </tbody>
        </table>
      </div>
      {outliersOpen ? (
        <RetrievalOutlierPanel
          isLoading={outliersQuery.isLoading}
          outliers={outliersQuery.data?.outliers ?? []}
          candidateRunId={candidateRunId}
        />
      ) : null}
    </section>
  );
}

/// Drill-down list — worst-case (hybrid - trigram) lifts in the
/// candidate run. Surfaces "which specific cases dragged the
/// hybrid mean down?" so an operator can click into one and
/// inspect the actual ranked legs. Lazily loaded — the parent
/// section only mounts this when the operator toggles the
/// outliers panel open.
function RetrievalOutlierPanel({
  isLoading,
  outliers,
  candidateRunId,
}: {
  isLoading: boolean;
  outliers: readonly RetrievalComparisonOutlier[];
  candidateRunId: string;
}) {
  const t = useTranslations("settings.evaluation.diff");
  return (
    <div className="mt-4 rounded-lg border border-divider bg-surface-inset p-3">
      <div className="text-2xs font-medium uppercase tracking-wide text-foreground-muted">
        {t("retrievalLiftDelta.outliersTitle")}
      </div>
      <p className="mt-1 text-xs text-foreground-muted">
        {t("retrievalLiftDelta.outliersDescription")}
      </p>
      {isLoading ? (
        <p className="mt-2 text-xs text-foreground-muted">
          {t("retrievalLiftDelta.outliersLoading")}
        </p>
      ) : outliers.length === 0 ? (
        <p className="mt-2 text-xs text-foreground-muted">
          {t("retrievalLiftDelta.outliersEmpty")}
        </p>
      ) : (
        <ul className="mt-2 space-y-1.5">
          {outliers.map((o) => (
            <li
              key={o.case_id}
              className="grid grid-cols-[1fr_auto_auto_auto_auto] items-baseline gap-3 rounded-md bg-surface-base px-2.5 py-1.5"
            >
              <Link
                href={`/evaluation/${encodeURIComponent(
                  candidateRunId,
                )}?case=${encodeURIComponent(o.case_id)}`}
                className="truncate font-medium text-foreground-strong underline-offset-2 hover:underline"
              >
                {o.case_key}
              </Link>
              <span className="text-2xs text-foreground-muted">
                {o.surface} · {o.axis}
              </span>
              <span className="tabular-nums text-foreground-muted">
                H {o.hybrid_score.toFixed(3)}
              </span>
              <span className="tabular-nums text-foreground-muted">
                T {o.trigram_score.toFixed(3)}
              </span>
              <span
                className={cn(
                  "tabular-nums font-medium",
                  o.case_lift < -1e-6
                    ? "text-danger-foreground"
                    : o.case_lift > 1e-6
                      ? "text-success-foreground"
                      : "text-foreground-muted",
                )}
              >
                {o.case_lift > 0 ? "+" : ""}
                {o.case_lift.toFixed(3)}
              </span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

function RetrievalLiftDeltaRow({
  row,
  t,
}: {
  row: RetrievalComparisonDelta;
  t: ReturnType<typeof useTranslations>;
}) {
  const tone = deltaTone(row.lift_delta);
  return (
    <tr className="border-t border-divider">
      <td className="px-4 py-2 font-medium">
        {t(`retrievalLiftDelta.surface.${surfaceI18nKey(row.surface)}`)}
      </td>
      <td className="px-4 py-2 text-foreground-muted">{row.axis}</td>
      <td className="px-4 py-2 text-end tabular-nums">
        {formatLiftCell(row.baseline_lift)}
      </td>
      <td className="px-4 py-2 text-end tabular-nums">
        {formatLiftCell(row.candidate_lift)}
      </td>
      <td
        className={cn(
          "px-4 py-2 text-end tabular-nums font-medium",
          TONE_FG[tone],
        )}
      >
        {formatLiftCell(row.lift_delta)}
      </td>
      <td className="px-4 py-2 text-end tabular-nums text-foreground-muted">
        {t("retrievalLiftDelta.pairedCountsCell", {
          baseline: row.baseline_paired_case_count,
          candidate: row.candidate_paired_case_count,
        })}
      </td>
    </tr>
  );
}

function formatLiftCell(value: number): string {
  // Same parity tolerance as the per-case + per-run aggregate
  // surfaces — `|x| < 1e-6` reads as `±0.000`. f64 round-trip
  // noise from `AVG(...)` over many cases can produce a near-
  // zero non-zero that would otherwise flicker tone.
  if (Math.abs(value) < 1e-6) return "±0.000";
  return (value > 0 ? "+" : "") + value.toFixed(3);
}

function surfaceI18nKey(surface: string): string {
  switch (surface) {
    case "verified_query":
      return "verifiedQuery";
    case "community_summary":
      return "communitySummary";
    case "knowledge_entry":
      return "knowledgeEntry";
    default:
      return surface;
  }
}

interface PerAxisRowProps {
  row: RunAxisSummary;
  t: ReturnType<typeof useTranslations>;
}

function PerAxisRow({ row, t }: PerAxisRowProps) {
  const tone = deltaTone(row.mean_delta);
  return (
    <tr className="border-t border-divider">
      <td className="px-4 py-2 font-medium">{row.axis}</td>
      <td className="px-4 py-2 text-end tabular-nums text-foreground-muted">
        {row.paired_case_count}
      </td>
      <td className="px-4 py-2 text-end tabular-nums">
        {row.baseline_mean.toFixed(3)}
      </td>
      <td className="px-4 py-2 text-end tabular-nums">
        {row.candidate_mean.toFixed(3)}
      </td>
      <td
        className={cn(
          "px-4 py-2 text-end tabular-nums font-medium",
          TONE_FG[tone],
        )}
      >
        {formatDelta(row.mean_delta)}
      </td>
      <td className="px-4 py-2 text-end tabular-nums">
        {row.win_rate_pct.toFixed(1)}%
      </td>
      <td className="px-4 py-2 text-end tabular-nums text-foreground-muted">
        {formatCohenD(t, row.cohen_d ?? undefined)}
      </td>
    </tr>
  );
}

function PerCaseRow({ row }: { row: RunMetricDelta }) {
  const tone = deltaTone(row.delta);
  return (
    <tr className={cn("border-t border-divider", TONE_BG[tone])}>
      <td className="px-4 py-2 font-medium">{row.case_key}</td>
      <td className="px-4 py-2 text-foreground-muted">{row.axis}</td>
      <td className="px-4 py-2 text-end tabular-nums">
        {row.baseline_score.toFixed(3)}
      </td>
      <td className="px-4 py-2 text-end tabular-nums">
        {row.candidate_score.toFixed(3)}
      </td>
      <td
        className={cn(
          "px-4 py-2 text-end tabular-nums font-medium",
          TONE_FG[tone],
        )}
      >
        {formatDelta(row.delta)}
      </td>
    </tr>
  );
}

function RunPicker({
  label,
  runs,
  value,
  onChange,
}: {
  label: string;
  runs: EvaluationRun[];
  value: string | null;
  onChange: (id: string | null) => void;
}) {
  return (
    <SettingsSelect
      label={label}
      value={value ?? ""}
      onChange={(e) => onChange(e.target.value || null)}
    >
      <option value="">—</option>
      {runs.map((r) => (
        <option key={r.id} value={r.id}>
          {r.name}
        </option>
      ))}
    </SettingsSelect>
  );
}

export default function EvaluationDiffPage() {
  const t = useTranslations("settings.evaluation.diff");
  const tCommon = useTranslations("common");
  const { isAdmin } = useAuth();
  const runsQuery = useEvaluationRuns();
  const [baselineId, setBaselineId] = useState<string | null>(null);
  const [candidateId, setCandidateId] = useState<string | null>(null);
  const diffQuery = useEvaluationRunDiff(baselineId, candidateId);

  const runs = useMemo(() => runsQuery.data?.items ?? [], [runsQuery.data]);

  if (!isAdmin) {
    return (
      <WorkbenchPageShell title={t("title")}>
        <EmptyState title={t("adminOnly")} />
      </WorkbenchPageShell>
    );
  }

  const pickerState: PageState = runsQuery.isLoading
    ? { kind: "loading" }
    : runsQuery.isError
      ? { kind: "error", onRetry: () => void runsQuery.refetch() }
      : { kind: "data" };

  const sameRunSelected =
    !!baselineId && !!candidateId && baselineId === candidateId;

  return (
    <WorkbenchPageShell title={t("title")}>
      <PageStateView
        state={pickerState}
        skeleton={<SkeletonList count={2} />}
        error={{
          title: tCommon("loadError.title"),
          description: tCommon("loadError.description"),
          retryLabel: tCommon("retry"),
        }}
      >
        <section className="mb-6 rounded-xl border border-divider bg-surface-base p-4">
          <Heading level={2} size={5}>
            {t("pickerTitle")}
          </Heading>
          <p className="mt-1 text-xs text-foreground-muted">
            {t("pickerDescription")}
          </p>
          <div className="mt-3 grid grid-cols-1 gap-3 md:grid-cols-2">
            <RunPicker
              label={t("baselineLabel")}
              runs={runs}
              value={baselineId}
              onChange={setBaselineId}
            />
            <RunPicker
              label={t("candidateLabel")}
              runs={runs}
              value={candidateId}
              onChange={setCandidateId}
            />
          </div>
          {sameRunSelected ? (
            <p className="mt-3 text-xs text-warning-foreground">
              {t("sameRunWarning")}
            </p>
          ) : null}
        </section>

        {!baselineId || !candidateId || sameRunSelected ? (
          <EmptyState
            title={t("emptyPick.title")}
            description={t("emptyPick.description")}
          />
        ) : diffQuery.isLoading ? (
          <SkeletonList count={4} />
        ) : diffQuery.isError ? (
          <EmptyState
            title={t("loadError.title")}
            description={
              diffQuery.error instanceof Error
                ? diffQuery.error.message
                : t("loadError.description")
            }
          />
        ) : !diffQuery.data ? null : (
          <>
            <section className="mb-6 rounded-xl border border-divider bg-surface-base p-4">
              <Heading level={2} size={5}>
                {t("perAxisTitle")}
              </Heading>
              <p className="mt-1 text-xs text-foreground-muted">
                {t("perAxisDescription")}
              </p>
              {diffQuery.data.per_axis.length === 0 ? (
                <EmptyState title={t("emptyAxis")} />
              ) : (
                <div className="mt-3 overflow-x-auto">
                  <table className="w-full text-sm">
                    <thead>
                      <tr className="text-start text-2xs font-medium uppercase tracking-wide text-foreground-muted">
                        <th className="px-4 py-2 text-start">{t("col.axis")}</th>
                        <th className="px-4 py-2 text-end">
                          {t("col.pairedCount")}
                        </th>
                        <th className="px-4 py-2 text-end">
                          {t("col.baselineMean")}
                        </th>
                        <th className="px-4 py-2 text-end">
                          {t("col.candidateMean")}
                        </th>
                        <th className="px-4 py-2 text-end">
                          {t("col.meanDelta")}
                        </th>
                        <th className="px-4 py-2 text-end">
                          {t("col.winRate")}
                        </th>
                        <th className="px-4 py-2 text-end">{t("col.cohenD")}</th>
                      </tr>
                    </thead>
                    <tbody>
                      {diffQuery.data.per_axis.map((row) => (
                        <PerAxisRow key={row.axis} row={row} t={t} />
                      ))}
                    </tbody>
                  </table>
                </div>
              )}
            </section>

            <RetrievalLiftRegressionBanner
              alerts={diffQuery.data.retrieval_lift_regressions ?? []}
            />

            <RetrievalLiftDeltaSection
              rows={diffQuery.data.retrieval_comparison_deltas ?? []}
              baselineRunId={diffQuery.data.baseline_run_id}
              candidateRunId={diffQuery.data.candidate_run_id}
            />

            <section className="rounded-xl border border-divider bg-surface-base p-4">
              <Heading level={2} size={5}>
                {t("perCaseTitle")}
              </Heading>
              <p className="mt-1 text-xs text-foreground-muted">
                {t("perCaseDescription", {
                  count: diffQuery.data.per_case.length,
                })}
              </p>
              {diffQuery.data.per_case.length === 0 ? (
                <EmptyState title={t("emptyCase")} />
              ) : (
                <div className="mt-3 overflow-x-auto">
                  <table className="w-full text-sm">
                    <thead>
                      <tr className="text-start text-2xs font-medium uppercase tracking-wide text-foreground-muted">
                        <th className="px-4 py-2 text-start">
                          {t("col.caseKey")}
                        </th>
                        <th className="px-4 py-2 text-start">{t("col.axis")}</th>
                        <th className="px-4 py-2 text-end">
                          {t("col.baseline")}
                        </th>
                        <th className="px-4 py-2 text-end">
                          {t("col.candidate")}
                        </th>
                        <th className="px-4 py-2 text-end">{t("col.delta")}</th>
                      </tr>
                    </thead>
                    <tbody>
                      {diffQuery.data.per_case.map((row) => (
                        <PerCaseRow
                          key={`${row.case_key}-${row.axis}`}
                          row={row}
                        />
                      ))}
                    </tbody>
                  </table>
                </div>
              )}
            </section>
          </>
        )}
      </PageStateView>
    </WorkbenchPageShell>
  );
}
