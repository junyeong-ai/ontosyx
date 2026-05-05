"use client";

import { useTranslations } from "next-intl";
import { use, useState } from "react";

import { useAuth } from "@/hooks/use-auth";
import {
  useEvaluationCases,
  useEvaluationMetrics,
  useEvaluationRun,
  useExecuteEvaluationCase,
  useJudgeEvaluationCase,
} from "@/hooks/api/use-evaluation";
import { SettingsPageShell } from "@/components/layout/settings-page-shell";
import { PageStateView } from "@/components/layout/page-state-view";
import type { PageState } from "@/components/layout/page-state";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { SkeletonList } from "@/components/ui/skeleton";
import { Heading } from "@/components/ui/heading";
import { SettingsInput } from "@/components/ui/form-input";
import { toast } from "@/components/ui/toast";
import { cn } from "@/lib/cn";
import type { EvaluationCase } from "@/types/evaluation";

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
  const [executeCaseKey, setExecuteCaseKey] = useState("");
  const [executeQuestion, setExecuteQuestion] = useState("");
  const canExecute =
    executeCaseKey.trim().length > 0 &&
    executeQuestion.trim().length > 0 &&
    !execute.isPending;
  const onExecute = () => {
    const caseKey = executeCaseKey.trim();
    execute.mutate(
      {
        caseKey,
        request: {
          kind: "translate_query",
          question: executeQuestion.trim(),
        },
      },
      {
        onSuccess: () => {
          toast.success(t("detail.execute.successToast", { caseKey }));
          setExecuteCaseKey("");
          setExecuteQuestion("");
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
          <div className="mt-3 grid grid-cols-1 gap-3 md:grid-cols-[1fr_3fr_auto] md:items-end">
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
                        "flex w-full items-baseline justify-between gap-3 px-4 py-2 text-left hover:bg-surface-inset",
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
