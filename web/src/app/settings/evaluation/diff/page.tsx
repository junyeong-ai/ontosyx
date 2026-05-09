"use client";

import { useTranslations } from "next-intl";
import { useMemo, useState } from "react";

import { useAuth } from "@/hooks/use-auth";
import {
  useEvaluationRunDiff,
  useEvaluationRuns,
} from "@/hooks/api/use-evaluation";
import { SettingsPageShell } from "@/components/layout/settings-page-shell";
import { PageStateView } from "@/components/layout/page-state-view";
import type { PageState } from "@/components/layout/page-state";
import { EmptyState } from "@/components/ui/empty-state";
import { SkeletonList } from "@/components/ui/skeleton";
import { Heading } from "@/components/ui/heading";
import { SettingsSelect } from "@/components/ui/form-input";
import { cn } from "@/lib/cn";
import type { components } from "@/types/api.generated";
import type {
  EvaluationRun,
  RunAxisSummary,
  RunMetricDelta,
} from "@/types/evaluation";

type RetrievalComparisonDelta =
  components["schemas"]["RetrievalComparisonDelta"];

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

/// Render the run-vs-run hybrid retrieval lift delta. Each row
/// is one (surface, axis) cell carrying both runs' average lift
/// + the inter-run delta. Tone keys off `lift_delta` so a
/// regression (candidate's lift dropped vs baseline) reads in
/// danger tone, an improvement in success tone, parity in
/// muted. Section disappears when neither run had any
/// retrieval_comparison cases (BE skips the field).
function RetrievalLiftDeltaSection({
  rows,
}: {
  rows: readonly RetrievalComparisonDelta[];
}) {
  const t = useTranslations("settings.evaluation.diff");
  if (rows.length === 0) return null;
  return (
    <section className="mb-6 rounded-xl border border-divider bg-surface-base p-4">
      <Heading level={2} size={5}>
        {t("retrievalLiftDelta.title")}
      </Heading>
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
    </section>
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
      <SettingsPageShell title={t("title")} subtitle={t("description")}>
        <EmptyState title={t("adminOnly")} />
      </SettingsPageShell>
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
    <SettingsPageShell title={t("title")} subtitle={t("description")}>
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

            <RetrievalLiftDeltaSection
              rows={diffQuery.data.retrieval_comparison_deltas ?? []}
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
    </SettingsPageShell>
  );
}
