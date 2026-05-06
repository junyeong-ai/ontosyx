"use client";

import { useTranslations } from "next-intl";
import { use, useState } from "react";

import { useAuth } from "@/hooks/use-auth";
import {
  useBulkUpsertEvaluationCases,
  useEvaluationCases,
  useEvaluationMetrics,
  useEvaluationRun,
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
  ExecuteEvaluationCaseRequest,
  ExecuteOperationKind,
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

export default function EvaluationDetailPage({
  params,
}: EvaluationDetailPageProps) {
  const { id } = use(params);
  const t = useTranslations("settings.evaluation");
  const tCommon = useTranslations("common");
  const { isAdmin } = useAuth();
  const runQuery = useEvaluationRun(id);
  const casesQuery = useEvaluationCases(id);
  const [activeCase, setActiveCase] = useState<EvaluationCase | null>(null);
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
    useState<ExecuteOperationKind>("translate_query");
  const [executeCaseKey, setExecuteCaseKey] = useState("");
  const [executeQuestion, setExecuteQuestion] = useState("");
  // Retrieval-only inputs. Surface alongside the question textbox
  // when the operator picks `retrieve_anchors`. Top-K defaults to
  // 10 (the platform's standard retrieval working set); anchors
  // accept the comma-separated `kind:logical_id` shape that
  // matches the BE wire shape.
  const [retrieveTopK, setRetrieveTopK] = useState(10);
  const [retrieveAnchorIds, setRetrieveAnchorIds] = useState("");
  const isRetrieve = executeKind === "retrieve_anchors";
  const canExecute =
    executeCaseKey.trim().length > 0 &&
    executeQuestion.trim().length > 0 &&
    !execute.isPending &&
    (!isRetrieve || retrieveTopK >= 1);
  const onExecute = () => {
    const caseKey = executeCaseKey.trim();
    const question = executeQuestion.trim();
    let request: ExecuteEvaluationCaseRequest;
    if (executeKind === "translate_query") {
      request = { kind: "translate_query", question };
    } else if (executeKind === "explain") {
      request = { kind: "explain", question };
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
                setExecuteKind(e.target.value as ExecuteOperationKind)
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
            </SettingsSelect>
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
          {isRetrieve ? (
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
                label={t("detail.execute.anchorIdsLabel")}
                placeholder={t("detail.execute.anchorIdsPlaceholder")}
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
                {cases.length}
              </span>
            </header>
            <p className="mb-3 text-xs text-foreground-muted">
              {t("detail.casesDescription")}
            </p>
            {cases.length === 0 ? (
              <EmptyState title={t("detail.noCases")} />
            ) : (
              <ul className="divide-y divide-divider rounded-xl border border-divider">
                {cases.map((c) => (
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
                        <div className="truncate font-medium">{c.case_key}</div>
                        {c.error ? (
                          <div className="truncate text-2xs text-danger-foreground">
                            {t("detail.errorLabel")}: {c.error}
                          </div>
                        ) : null}
                      </div>
                      <div className="shrink-0 text-2xs text-foreground-muted tabular-nums">
                        {typeof c.latency_ms === "number"
                          ? t("detail.latencyMs", { ms: c.latency_ms })
                          : t("detail.noLatency")}
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
