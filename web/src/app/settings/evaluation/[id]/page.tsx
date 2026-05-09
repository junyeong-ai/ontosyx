"use client";

import Link from "next/link";
import { useTranslations } from "next-intl";
import { use, useState } from "react";

import { useAuth } from "@/hooks/use-auth";
import {
  useBulkUpsertEvaluationCases,
  useEvaluationCases,
  useEvaluationMetrics,
  useEvaluationRun,
  useEvaluationRunSummary,
  useExecuteEvaluationCase,
  useJudgeEvaluationCase,
  useJudgeSafetyEvaluationCase,
  usePromoteCaseToDataset,
} from "@/hooks/api/use-evaluation";
import { usePrompt } from "@/components/ui/prompt-dialog";
import { SettingsPageShell } from "@/components/layout/settings-page-shell";
import { PageStateView } from "@/components/layout/page-state-view";
import type { PageState } from "@/components/layout/page-state";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { SkeletonList } from "@/components/ui/skeleton";
import { Heading } from "@/components/ui/heading";
import { FormTextarea, SettingsInput, SettingsSelect } from "@/components/ui/form-input";
import { toast } from "@/components/ui/toast";
import { cn } from "@/lib/cn";
import type {
  BulkUpsertEvaluationCaseEntry,
  EvaluationCase,
  EvaluationCaseInput,
  RunSummary,
} from "@/types/evaluation";

/**
 * Parse a textarea blob as either a JSON array of bulk entries or
 * JSONL (one entry per line). Returns the parsed entries on
 * success, `null` when neither shape lands.
 */
function parseBulkInput(
  raw: string,
): BulkUpsertEvaluationCaseEntry[] | null {
  const trimmed = raw.trim();
  if (trimmed.length === 0) return [];
  // Try JSON array first.
  if (trimmed.startsWith("[")) {
    try {
      const parsed = JSON.parse(trimmed) as BulkUpsertEvaluationCaseEntry[];
      if (!Array.isArray(parsed)) return null;
      return parsed;
    } catch {
      return null;
    }
  }
  // Otherwise JSONL — one JSON object per non-blank line.
  const out: BulkUpsertEvaluationCaseEntry[] = [];
  for (const line of trimmed.split(/\r?\n/)) {
    const t = line.trim();
    if (t.length === 0) continue;
    try {
      out.push(JSON.parse(t) as BulkUpsertEvaluationCaseEntry);
    } catch {
      return null;
    }
  }
  return out;
}

interface EvaluationDetailPageProps {
  params: Promise<{ id: string }>;
}

type CaseStatus = "pending" | "executed" | "failed";

/// `caseStatus` — derives a status string from the case's
/// `error` / `actual` / `latency_ms` fields. The triplet
/// (`pending` / `executed` / `failed`) is what the case-list
/// pill renders, so the FE doesn't have to plumb a fourth
/// status field through the wire shape — derivation lives in
/// one place.
function caseStatus(c: EvaluationCase): CaseStatus {
  if (c.error) return "failed";
  if (c.actual !== undefined && c.actual !== null) return "executed";
  return "pending";
}

/// Filter sentinel — `null` means "show every status", any
/// concrete `CaseStatus` narrows the list. Local UI state, not
/// persisted; the case list is short-lived enough that resetting
/// to "all" on navigation is the natural reset.
type CaseStatusFilter = CaseStatus | null;

const STATUS_PILL_CLASS: Record<
  ReturnType<typeof caseStatus>,
  { bg: string; fg: string; ring: string }
> = {
  pending: {
    bg: "bg-surface-inset",
    fg: "text-foreground-muted",
    ring: "ring-divider",
  },
  executed: {
    bg: "bg-success-surface",
    fg: "text-success-foreground",
    ring: "ring-success-border",
  },
  failed: {
    bg: "bg-danger-surface",
    fg: "text-danger-foreground",
    ring: "ring-danger-border",
  },
};

function CaseStatusPill({
  status,
  label,
}: {
  status: ReturnType<typeof caseStatus>;
  label: string;
}) {
  const tone = STATUS_PILL_CLASS[status];
  return (
    <span
      className={cn(
        "inline-flex shrink-0 items-center rounded-full px-2 py-0.5 text-2xs font-medium ring-1",
        tone.bg,
        tone.fg,
        tone.ring,
      )}
    >
      {label}
    </span>
  );
}

/// `<SummaryCard>` — header tile for the run-detail page. Reads
/// the run summary endpoint and renders three counters
/// (total / judged / failed) plus a per-axis mean strip. Order
/// of the per-axis chips matches the BE's alphabetic sort —
/// safety axes ride together at the bottom because of the
/// `safety.` prefix.
function SummaryCard({
  summary,
  t,
}: {
  summary: RunSummary;
  t: ReturnType<typeof useTranslations>;
}) {
  const { total_cases, judged_cases, failed_cases, axis_means } = summary;
  return (
    <section className="mb-6 rounded-xl border border-divider bg-surface-base p-4">
      <Heading level={2} size={5}>
        {t("detail.summary.title")}
      </Heading>
      <p className="mt-1 text-xs text-foreground-muted">
        {t("detail.summary.description")}
      </p>
      <div className="mt-3 grid grid-cols-3 gap-3">
        <div className="rounded-lg border border-divider bg-surface-inset px-3 py-2">
          <div className="text-2xs text-foreground-muted">
            {t("detail.summary.totalLabel")}
          </div>
          <div className="text-lg font-medium tabular-nums text-foreground-strong">
            {total_cases}
          </div>
        </div>
        <div className="rounded-lg border border-divider bg-surface-inset px-3 py-2">
          <div className="text-2xs text-foreground-muted">
            {t("detail.summary.judgedLabel")}
          </div>
          <div className="text-lg font-medium tabular-nums text-foreground-strong">
            {judged_cases}
            <span className="ms-1 text-2xs text-foreground-muted">
              / {total_cases}
            </span>
          </div>
        </div>
        <div
          className={cn(
            "rounded-lg border px-3 py-2",
            failed_cases > 0
              ? "border-danger-border bg-danger-surface"
              : "border-divider bg-surface-inset",
          )}
        >
          <div
            className={cn(
              "text-2xs",
              failed_cases > 0
                ? "text-danger-foreground"
                : "text-foreground-muted",
            )}
          >
            {t("detail.summary.failedLabel")}
          </div>
          <div
            className={cn(
              "text-lg font-medium tabular-nums",
              failed_cases > 0
                ? "text-danger-foreground"
                : "text-foreground-strong",
            )}
          >
            {failed_cases}
          </div>
        </div>
      </div>
      {axis_means.length > 0 && (
        <div className="mt-3 flex flex-wrap gap-2">
          {axis_means.map((a) => (
            <div
              key={a.axis}
              className="rounded-md border border-divider bg-surface-inset px-2.5 py-1"
            >
              <span className="text-2xs text-foreground-muted">
                {a.axis}
              </span>
              <span className="ms-2 text-xs font-medium tabular-nums text-foreground-strong">
                {a.mean.toFixed(3)}
              </span>
              <span className="ms-1 text-2xs text-foreground-muted tabular-nums">
                ({a.count})
              </span>
            </div>
          ))}
        </div>
      )}
    </section>
  );
}

export default function EvaluationDetailPage({
  params,
}: EvaluationDetailPageProps) {
  const { id } = use(params);
  const t = useTranslations("settings.evaluation");
  const tCommon = useTranslations("common");
  const { isAdmin } = useAuth();
  const runQuery = useEvaluationRun(id);
  const casesQuery = useEvaluationCases(id);
  const summaryQuery = useEvaluationRunSummary(id);
  const [activeCase, setActiveCase] = useState<EvaluationCase | null>(null);
  const [statusFilter, setStatusFilter] = useState<CaseStatusFilter>(null);
  const metricsQuery = useEvaluationMetrics(activeCase?.id ?? null);
  const execute = useExecuteEvaluationCase(id);
  const judge = useJudgeEvaluationCase();
  const safetyJudge = useJudgeSafetyEvaluationCase();
  const promote = usePromoteCaseToDataset();
  const prompt = usePrompt();
  const bulk = useBulkUpsertEvaluationCases(id);
  const [bulkText, setBulkText] = useState("");
  const onBulk = () => {
    const parsed = parseBulkInput(bulkText);
    if (parsed === null) {
      toast.error(t("detail.bulk.parseError"));
      return;
    }
    bulk.mutate(
      { cases: parsed },
      {
        onSuccess: (res) => {
          if (res.errors.length === 0) {
            toast.success(
              t("detail.bulk.successToast", { count: res.upserted_count }),
            );
            setBulkText("");
          } else {
            toast.error(
              t("detail.bulk.partialToast", {
                ok: res.upserted_count,
                failed: res.errors.length,
              }),
            );
          }
        },
        onError: (err) => {
          toast.error(
            t("detail.bulk.errorToast", {
              error: err instanceof Error ? err.message : String(err),
            }),
          );
        },
      },
    );
  };
  const canBulk = bulkText.trim().length > 0 && !bulk.isPending;
  const [executeKind, setExecuteKind] =
    useState<EvaluationCaseInput["kind"]>("translate_query");
  const [executeCaseKey, setExecuteCaseKey] = useState("");
  const [executeQuestion, setExecuteQuestion] = useState("");
  // Retrieval-only inputs. Surface alongside the question textbox
  // when the operator picks `retrieve_anchors` or
  // `retrieval_comparison`. Top-K defaults to 10 (the platform's
  // standard retrieval working set); expected ids accept the
  // comma-separated `kind:logical_id` shape that matches the BE
  // wire shape.
  const [retrieveTopK, setRetrieveTopK] = useState(10);
  const [retrieveAnchorIds, setRetrieveAnchorIds] = useState("");
  const [comparisonSurface, setComparisonSurface] = useState<
    "verified_query" | "community_summary" | "knowledge_entry"
  >("verified_query");
  const isRetrieve = executeKind === "retrieve_anchors";
  const isComparison = executeKind === "retrieval_comparison";
  const needsTopK = isRetrieve || isComparison;
  const canExecute =
    executeCaseKey.trim().length > 0 &&
    executeQuestion.trim().length > 0 &&
    !execute.isPending &&
    (!needsTopK || retrieveTopK >= 1);
  const onExecute = () => {
    const caseKey = executeCaseKey.trim();
    const question = executeQuestion.trim();
    let request: EvaluationCaseInput;
    if (executeKind === "translate_query") {
      request = { kind: "translate_query", question };
    } else if (executeKind === "explain") {
      request = { kind: "explain", question };
    } else if (executeKind === "retrieval_comparison") {
      const expected_ids = retrieveAnchorIds
        .split(/[\s,]+/)
        .map((s) => s.trim())
        .filter((s) => s.length > 0);
      request = {
        kind: "retrieval_comparison",
        question,
        surface: comparisonSurface,
        top_k: retrieveTopK,
        expected_ids,
      };
    } else {
      const expected_anchor_ids = retrieveAnchorIds
        .split(/[\s,]+/)
        .map((s) => s.trim())
        .filter((s) => s.length > 0);
      request = {
        kind: "retrieve_anchors",
        question,
        top_k: retrieveTopK,
        expected_anchor_ids,
      };
    }
    execute.mutate(
      {
        caseKey,
        request,
      },
      {
        onSuccess: () => {
          toast.success(t("detail.execute.successToast", { caseKey }));
          setExecuteCaseKey("");
          setExecuteQuestion("");
          setRetrieveAnchorIds("");
        },
        onError: (err) => {
          toast.error(
            t("detail.execute.errorToast", {
              error: err instanceof Error ? err.message : String(err),
            }),
          );
        },
      },
    );
  };

  if (!isAdmin) {
    return (
      <SettingsPageShell title={t("title")} subtitle={t("description")}>
        <EmptyState title={t("adminOnly")} />
      </SettingsPageShell>
    );
  }

  const pageState: PageState = runQuery.isLoading
    ? { kind: "loading" }
    : runQuery.isError
      ? { kind: "error", onRetry: () => void runQuery.refetch() }
      : { kind: "data" };

  const run = runQuery.data;
  const cases = casesQuery.data ?? [];
  const filteredCases =
    statusFilter === null
      ? cases
      : cases.filter((c) => caseStatus(c) === statusFilter);
  // First-load convenience — pin the inspector to the first case
  // when nothing is selected yet. The user can switch by clicking
  // any row in the cases pane.
  const selectedCase =
    activeCase ?? (cases.length > 0 ? (cases[0] ?? null) : null);
  const canJudge = !!selectedCase?.actual && !judge.isPending;
  const canSafetyJudge =
    !!selectedCase?.actual && !safetyJudge.isPending;
  const onJudge = () => {
    if (!selectedCase) return;
    judge.mutate(selectedCase.id, {
      onSuccess: (metrics) => {
        toast.success(
          t("detail.judge.successToast", { count: metrics.length }),
        );
      },
      onError: (err) => {
        toast.error(
          t("detail.judge.errorToast", {
            error: err instanceof Error ? err.message : String(err),
          }),
        );
      },
    });
  };
  const onSafetyJudge = () => {
    if (!selectedCase) return;
    safetyJudge.mutate(selectedCase.id, {
      onSuccess: (metrics) => {
        toast.success(
          t("detail.safetyJudge.successToast", { count: metrics.length }),
        );
      },
      onError: (err) => {
        toast.error(
          t("detail.safetyJudge.errorToast", {
            error: err instanceof Error ? err.message : String(err),
          }),
        );
      },
    });
  };
  const canPromote = !!selectedCase && !promote.isPending;
  /// Re-runs a failed case using its persisted `input` envelope.
  /// The `EvaluationCase.input` already carries the full
  /// `EvaluationCaseInput` shape (kind + question + per-
  /// kind fields), so retry is a one-line dispatch — no operator
  /// re-input needed. The case_key stays the same so the natural-
  /// key UPSERT (`run_id, case_key`) replaces the failed row in
  /// place; metrics from the prior failed attempt cascade-delete
  /// when the FK fires.
  const onRetryCase = (c: EvaluationCase, evt: React.MouseEvent) => {
    // The retry button lives inside the case row's `<button>`
    // (selecting the case) — stop propagation so the click
    // doesn't also flip `selectedCase`. Operators clicking the
    // body row vs the retry pill is the discriminator.
    evt.stopPropagation();
    let parsed: EvaluationCaseInput;
    try {
      parsed = c.input as EvaluationCaseInput;
    } catch {
      toast.error(t("detail.retry.parseError"));
      return;
    }
    execute.mutate(
      { caseKey: c.case_key, request: parsed },
      {
        onSuccess: () => {
          toast.success(
            t("detail.retry.successToast", { caseKey: c.case_key }),
          );
        },
        onError: (err) => {
          toast.error(
            t("detail.retry.errorToast", {
              error: err instanceof Error ? err.message : String(err),
            }),
          );
        },
      },
    );
  };
  const onPromote = async () => {
    if (!selectedCase) return;
    // Two-step prompt: dataset id then promotion mode. The
    // codebase's `usePrompt` is single-field, so the second
    // toggle defaults conservatively (`use_actual_as_expected =
    // false`). Operators who want the captured `actual` as the
    // golden expected answer can flip the toggle on the dataset
    // detail page after the item lands.
    const datasetId = await prompt({
      title: t("detail.promote.title"),
      description: t("detail.promote.description"),
      placeholder: t("detail.promote.datasetIdPlaceholder"),
    });
    if (!datasetId) return; // operator cancelled
    promote.mutate(
      {
        caseId: selectedCase.id,
        request: { dataset_id: datasetId.trim() },
      },
      {
        onSuccess: () => {
          toast.success(t("detail.promote.successToast"));
        },
        onError: (err) => {
          toast.error(
            t("detail.promote.errorToast", {
              error: err instanceof Error ? err.message : String(err),
            }),
          );
        },
      },
    );
  };

  return (
    <SettingsPageShell
      title={run?.name ?? t("title")}
      subtitle={run?.description ?? t("description")}
    >
      <PageStateView
        state={pageState}
        skeleton={<SkeletonList count={4} />}
        error={{
          title: tCommon("loadError.title"),
          description: tCommon("loadError.description"),
          retryLabel: tCommon("retry"),
        }}
      >
        {run?.fingerprint?.dataset_id ? (
          <div className="mb-4 flex items-center gap-2 rounded-md border border-divider bg-surface-inset px-3 py-2 text-xs">
            <span className="text-foreground-muted">
              {t("detail.lineage.fromDataset")}
            </span>
            <Link
              href={`/settings/evaluation/datasets/${encodeURIComponent(run.fingerprint.dataset_id)}`}
              className="font-mono text-2xs text-brand-foreground hover:underline"
            >
              {run.fingerprint.dataset_id}
            </Link>
          </div>
        ) : null}

        {summaryQuery.data ? (
          <SummaryCard summary={summaryQuery.data} t={t} />
        ) : null}

        <section className="mb-6 rounded-xl border border-divider bg-surface-base p-4">
          <Heading level={2} size={5}>
            {t("detail.execute.title")}
          </Heading>
          <p className="mt-1 text-xs text-foreground-muted">
            {t("detail.execute.description")}
          </p>
          <div className="mt-3 grid grid-cols-1 gap-3 md:grid-cols-[2fr_1fr_3fr_auto] md:items-end">
            <SettingsSelect
              label={t("detail.execute.kindLabel")}
              value={executeKind}
              onChange={(e) =>
                setExecuteKind(e.target.value as EvaluationCaseInput["kind"])
              }
            >
              <option value="translate_query">
                {t("detail.execute.kindOption.translateQuery")}
              </option>
              <option value="explain">
                {t("detail.execute.kindOption.explain")}
              </option>
              <option value="retrieve_anchors">
                {t("detail.execute.kindOption.retrieveAnchors")}
              </option>
              <option value="retrieval_comparison">
                {t("detail.execute.kindOption.retrievalComparison")}
              </option>
            </SettingsSelect>
            {isComparison ? (
              <SettingsSelect
                label={t("detail.execute.surfaceLabel")}
                value={comparisonSurface}
                onChange={(e) =>
                  setComparisonSurface(
                    e.target.value as
                      | "verified_query"
                      | "community_summary"
                      | "knowledge_entry",
                  )
                }
              >
                <option value="verified_query">
                  {t("detail.execute.surfaceOption.verifiedQuery")}
                </option>
                <option value="community_summary">
                  {t("detail.execute.surfaceOption.communitySummary")}
                </option>
                <option value="knowledge_entry">
                  {t("detail.execute.surfaceOption.knowledgeEntry")}
                </option>
              </SettingsSelect>
            ) : null}
            <SettingsInput
              label={t("detail.execute.caseKeyLabel")}
              placeholder={t("detail.execute.caseKeyPlaceholder")}
              value={executeCaseKey}
              onChange={(e) => setExecuteCaseKey(e.target.value)}
            />
            <SettingsInput
              label={t("detail.execute.questionLabel")}
              placeholder={t("detail.execute.questionPlaceholder")}
              value={executeQuestion}
              onChange={(e) => setExecuteQuestion(e.target.value)}
            />
            <Button
              type="button"
              onClick={onExecute}
              disabled={!canExecute}
              loading={execute.isPending}
            >
              {execute.isPending
                ? t("detail.execute.submitting")
                : t("detail.execute.submit")}
            </Button>
          </div>
          {needsTopK ? (
            <div className="mt-3 grid grid-cols-1 gap-3 md:grid-cols-[1fr_3fr] md:items-end">
              <SettingsInput
                label={t("detail.execute.topKLabel")}
                type="number"
                min={1}
                max={100}
                value={retrieveTopK}
                onChange={(e) => {
                  const next = Number.parseInt(e.target.value, 10);
                  if (Number.isFinite(next))
                    setRetrieveTopK(Math.max(1, Math.min(100, next)));
                }}
              />
              <SettingsInput
                label={
                  isComparison
                    ? t("detail.execute.expectedIdsLabel")
                    : t("detail.execute.anchorIdsLabel")
                }
                placeholder={
                  isComparison
                    ? t("detail.execute.expectedIdsPlaceholder")
                    : t("detail.execute.anchorIdsPlaceholder")
                }
                value={retrieveAnchorIds}
                onChange={(e) => setRetrieveAnchorIds(e.target.value)}
              />
            </div>
          ) : null}
        </section>

        <section className="mb-6 rounded-xl border border-divider bg-surface-base p-4">
          <Heading level={2} size={5}>
            {t("detail.bulk.title")}
          </Heading>
          <p className="mt-1 text-xs text-foreground-muted">
            {t("detail.bulk.description")}
          </p>
          <div className="mt-3 flex flex-col gap-3">
            <FormTextarea
              value={bulkText}
              onChange={(e) => setBulkText(e.target.value)}
              placeholder={t("detail.bulk.placeholder")}
              spellCheck={false}
              rows={6}
              className="w-full font-mono"
            />
            <div className="flex justify-end">
              <Button
                type="button"
                onClick={onBulk}
                disabled={!canBulk}
                loading={bulk.isPending}
              >
                {bulk.isPending
                  ? t("detail.bulk.submitting")
                  : t("detail.bulk.submit")}
              </Button>
            </div>
          </div>
        </section>

        <div className="grid grid-cols-1 gap-6 lg:grid-cols-2">
          <section>
            <header className="mb-2 flex items-baseline justify-between">
              <Heading level={2} size={5}>
                {t("detail.casesTitle")}
              </Heading>
              <span className="text-2xs text-foreground-muted tabular-nums">
                {statusFilter === null
                  ? cases.length
                  : t("detail.caseFilter.countLabel", {
                      shown: filteredCases.length,
                      total: cases.length,
                    })}
              </span>
            </header>
            <p className="mb-3 text-xs text-foreground-muted">
              {t("detail.casesDescription")}
            </p>
            {cases.length === 0 ? null : (
              <div
                className="mb-3 inline-flex flex-wrap items-center gap-1 rounded-lg bg-surface-inset p-0.5"
                role="tablist"
                aria-label={t("detail.caseFilter.ariaLabel")}
              >
                {(
                  [null, "executed", "failed", "pending"] as const
                ).map((option) => {
                  const isActive = statusFilter === option;
                  const optionCount =
                    option === null
                      ? cases.length
                      : cases.filter((c) => caseStatus(c) === option).length;
                  const labelKey =
                    option === null
                      ? "all"
                      : (option as CaseStatus);
                  return (
                    <button
                      key={option ?? "all"}
                      type="button"
                      role="tab"
                      aria-selected={isActive}
                      onClick={() => setStatusFilter(option)}
                      className={cn(
                        "rounded-md px-2.5 py-1 text-xs transition-colors duration-[var(--duration-base)] ease-[var(--ease-out)]",
                        isActive
                          ? "bg-surface-base text-foreground-strong shadow-1-strong"
                          : "text-foreground-muted hover:text-foreground-strong",
                      )}
                    >
                      {t(`detail.caseFilter.${labelKey}`)}
                      <span className="ms-1.5 tabular-nums text-2xs text-foreground-muted">
                        {optionCount}
                      </span>
                    </button>
                  );
                })}
              </div>
            )}
            {cases.length === 0 ? (
              <EmptyState title={t("detail.noCases")} />
            ) : filteredCases.length === 0 ? (
              <EmptyState
                title={t("detail.caseFilter.emptyForFilter")}
              />
            ) : (
              <ul className="divide-y divide-divider rounded-xl border border-divider">
                {filteredCases.map((c) => (
                  <li key={c.id}>
                    <button
                      type="button"
                      onClick={() => setActiveCase(c)}
                      className={cn(
                        "flex w-full items-baseline justify-between gap-3 px-4 py-2 text-start hover:bg-surface-inset",
                        selectedCase?.id === c.id && "bg-surface-inset",
                      )}
                    >
                      <div className="min-w-0 flex-1">
                        <div className="flex items-center gap-2">
                          <span className="truncate font-medium">
                            {c.case_key}
                          </span>
                          <CaseStatusPill
                            status={caseStatus(c)}
                            label={t(
                              `detail.caseStatus.${caseStatus(c)}`,
                            )}
                          />
                        </div>
                        {c.error ? (
                          <div className="truncate text-2xs text-danger-foreground">
                            {t("detail.errorLabel")}: {c.error}
                          </div>
                        ) : null}
                      </div>
                      <div className="flex shrink-0 items-center gap-2 text-2xs text-foreground-muted tabular-nums">
                        {caseStatus(c) === "failed" ? (
                          <button
                            type="button"
                            onClick={(evt) => onRetryCase(c, evt)}
                            disabled={execute.isPending}
                            className="rounded-md border border-divider bg-surface-base px-2 py-0.5 font-medium text-foreground-strong hover:bg-surface-raised disabled:opacity-50"
                          >
                            {t("detail.retry.label")}
                          </button>
                        ) : null}
                        <span>
                          {typeof c.latency_ms === "number"
                            ? t("detail.latencyMs", { ms: c.latency_ms })
                            : t("detail.noLatency")}
                        </span>
                      </div>
                    </button>
                  </li>
                ))}
              </ul>
            )}
          </section>

          <section>
            <header className="mb-2 flex items-baseline justify-between gap-3">
              <Heading level={2} size={5}>
                {t("detail.metricsTitle")}
              </Heading>
              <div className="flex items-center gap-2">
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  onClick={onJudge}
                  disabled={!canJudge}
                  loading={judge.isPending}
                  title={
                    selectedCase?.actual
                      ? undefined
                      : t("detail.judge.noActual")
                  }
                >
                  {judge.isPending
                    ? t("detail.judge.submitting")
                    : t("detail.judge.label")}
                </Button>
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  onClick={onSafetyJudge}
                  disabled={!canSafetyJudge}
                  loading={safetyJudge.isPending}
                  title={
                    selectedCase?.actual
                      ? undefined
                      : t("detail.safetyJudge.noActual")
                  }
                >
                  {safetyJudge.isPending
                    ? t("detail.safetyJudge.submitting")
                    : t("detail.safetyJudge.label")}
                </Button>
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  onClick={onPromote}
                  disabled={!canPromote}
                  loading={promote.isPending}
                  title={
                    selectedCase
                      ? undefined
                      : t("detail.promote.noCase")
                  }
                >
                  {promote.isPending
                    ? t("detail.promote.submitting")
                    : t("detail.promote.label")}
                </Button>
                <span className="text-2xs text-foreground-muted tabular-nums">
                  {(metricsQuery.data ?? []).length}
                </span>
              </div>
            </header>
            <p className="mb-3 text-xs text-foreground-muted">
              {t("detail.metricsDescription")}
            </p>
            {!selectedCase ? (
              <EmptyState title={t("detail.noCases")} />
            ) : (metricsQuery.data ?? []).length === 0 ? (
              <EmptyState title={t("detail.noMetrics")} />
            ) : (
              <ul className="divide-y divide-divider rounded-xl border border-divider">
                {(metricsQuery.data ?? []).map((m) => (
                  <li
                    key={m.id}
                    className="flex items-baseline justify-between gap-3 px-4 py-2"
                  >
                    <div className="min-w-0 flex-1">
                      <div className="truncate font-medium">{m.name}</div>
                      {m.reasoning ? (
                        <div className="mt-0.5 line-clamp-2 text-2xs text-foreground-muted">
                          {m.reasoning}
                        </div>
                      ) : null}
                    </div>
                    <div className="shrink-0 text-sm font-medium tabular-nums">
                      {m.score.toFixed(3)}
                    </div>
                  </li>
                ))}
              </ul>
            )}
          </section>
        </div>
      </PageStateView>
    </SettingsPageShell>
  );
}
