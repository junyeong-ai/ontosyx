"use client";

import { useTranslations } from "next-intl";
import { use, useState } from "react";

import { useAuth } from "@/hooks/use-auth";
import {
  useEvaluationDataset,
  useEvaluationDatasetItems,
  useReplaceEvaluationDatasetItems,
} from "@/hooks/api/use-evaluation";
import { SettingsPageShell } from "@/components/layout/settings-page-shell";
import { PageStateView } from "@/components/layout/page-state-view";
import type { PageState } from "@/components/layout/page-state";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { SkeletonList } from "@/components/ui/skeleton";
import { Heading } from "@/components/ui/heading";
import { FormTextarea } from "@/components/ui/form-input";
import { toast } from "@/components/ui/toast";
import type { UpsertEvaluationDatasetItemEntry } from "@/types/evaluation";

interface DatasetDetailPageProps {
  params: Promise<{ id: string }>;
}

/// Truncate a JSON-shaped value to a short prose summary for the
/// items table. Heuristic: if it's an object with a `question`
/// field (the `ExecuteEvaluationCaseRequest` shape every dataset
/// item carries), surface that. Otherwise stringify with a hard
/// truncation. Keeps the table readable without forcing the
/// operator to expand each row to inspect the input.
function summariseInput(input: unknown, max = 120): string {
  if (input && typeof input === "object" && "question" in input) {
    const q = (input as { question?: unknown }).question;
    if (typeof q === "string") {
      return q.length > max ? `${q.slice(0, max)}…` : q;
    }
  }
  const json = JSON.stringify(input);
  if (!json) return "—";
  return json.length > max ? `${json.slice(0, max)}…` : json;
}

function inputKind(input: unknown): string {
  if (input && typeof input === "object" && "kind" in input) {
    const k = (input as { kind?: unknown }).kind;
    if (typeof k === "string") return k;
  }
  return "—";
}

/// Parse a textarea blob as either a JSON array of dataset-item
/// entries or JSONL (one entry per non-blank line). Mirrors the
/// case-bulk parser shape on the run-detail page so operators
/// see consistent semantics across surfaces. Returns `null`
/// when neither shape lands cleanly so the caller surfaces a
/// "could not parse" toast.
function parseBulkInput(
  raw: string,
): UpsertEvaluationDatasetItemEntry[] | null {
  const trimmed = raw.trim();
  if (trimmed.length === 0) return [];
  if (trimmed.startsWith("[")) {
    try {
      const parsed = JSON.parse(trimmed) as UpsertEvaluationDatasetItemEntry[];
      if (!Array.isArray(parsed)) return null;
      return parsed;
    } catch {
      return null;
    }
  }
  const out: UpsertEvaluationDatasetItemEntry[] = [];
  for (const line of trimmed.split(/\r?\n/)) {
    const t = line.trim();
    if (t.length === 0) continue;
    try {
      out.push(JSON.parse(t) as UpsertEvaluationDatasetItemEntry);
    } catch {
      return null;
    }
  }
  return out;
}

export default function EvaluationDatasetDetailPage({
  params,
}: DatasetDetailPageProps) {
  const { id } = use(params);
  const t = useTranslations("settings.evaluation.datasets");
  const tCommon = useTranslations("common");
  const { isAdmin } = useAuth();
  const datasetQuery = useEvaluationDataset(id);
  const itemsQuery = useEvaluationDatasetItems(id);
  const replace = useReplaceEvaluationDatasetItems(id);
  const [bulkText, setBulkText] = useState("");
  const onBulk = () => {
    const parsed = parseBulkInput(bulkText);
    if (parsed === null) {
      toast.error(t("detail.bulk.parseError"));
      return;
    }
    replace.mutate(
      { items: parsed },
      {
        onSuccess: (res) => {
          toast.success(
            t("detail.bulk.successToast", { count: res.item_count }),
          );
          setBulkText("");
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
  const canBulk = bulkText.trim().length > 0 && !replace.isPending;

  if (!isAdmin) {
    return (
      <SettingsPageShell title={t("detail.title")} subtitle={t("description")}>
        <EmptyState title={t("adminOnly")} />
      </SettingsPageShell>
    );
  }

  const pageState: PageState = datasetQuery.isLoading || itemsQuery.isLoading
    ? { kind: "loading" }
    : datasetQuery.isError || itemsQuery.isError
      ? {
          kind: "error",
          onRetry: () => {
            void datasetQuery.refetch();
            void itemsQuery.refetch();
          },
        }
      : { kind: "data" };

  const dataset = datasetQuery.data;
  const items = itemsQuery.data ?? [];

  return (
    <SettingsPageShell
      title={dataset?.name ?? t("detail.title")}
      subtitle={dataset?.description || t("description")}
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
        {dataset ? (
          <section className="mb-6 rounded-xl border border-divider bg-surface-base p-4">
            <Heading level={2} size={5}>
              {t("detail.headerTitle")}
            </Heading>
            <dl className="mt-3 grid grid-cols-1 gap-x-6 gap-y-2 sm:grid-cols-[max-content_1fr] text-sm">
              <dt className="text-foreground-muted">{t("col.name")}</dt>
              <dd className="font-medium">{dataset.name}</dd>
              <dt className="text-foreground-muted">{t("col.id")}</dt>
              <dd className="font-mono text-2xs text-foreground-muted">
                {dataset.id}
              </dd>
              <dt className="text-foreground-muted">{t("col.description")}</dt>
              <dd className="text-foreground-muted">
                {dataset.description || (
                  <span className="text-foreground-subtle">—</span>
                )}
              </dd>
            </dl>
          </section>
        ) : null}

        <section className="mb-6 rounded-xl border border-divider bg-surface-base p-4">
          <Heading level={2} size={5}>
            {t("detail.bulk.title")}
          </Heading>
          <p className="mt-1 text-xs text-warning-foreground">
            {t("detail.bulk.replaceWarning")}
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
                loading={replace.isPending}
              >
                {replace.isPending
                  ? t("detail.bulk.submitting")
                  : t("detail.bulk.submit")}
              </Button>
            </div>
          </div>
        </section>

        <section className="rounded-xl border border-divider bg-surface-base p-4">
          <header className="mb-3 flex items-baseline justify-between">
            <Heading level={2} size={5}>
              {t("detail.itemsTitle")}
            </Heading>
            <span className="text-2xs text-foreground-muted tabular-nums">
              {items.length}
            </span>
          </header>
          <p className="mb-3 text-xs text-foreground-muted">
            {t("detail.itemsDescription")}
          </p>
          {items.length === 0 ? (
            <EmptyState title={t("detail.emptyItems")} />
          ) : (
            <div className="overflow-x-auto">
              <table className="w-full text-sm">
                <thead>
                  <tr className="border-b border-divider text-start text-2xs font-medium uppercase tracking-wide text-foreground-muted">
                    <th className="px-3 py-2 text-start">
                      {t("detail.col.itemKey")}
                    </th>
                    <th className="px-3 py-2 text-start">
                      {t("detail.col.kind")}
                    </th>
                    <th className="px-3 py-2 text-start">
                      {t("detail.col.input")}
                    </th>
                    <th className="px-3 py-2 text-start">
                      {t("detail.col.expected")}
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {items.map((it) => (
                    <tr
                      key={it.id}
                      className="border-b border-divider last:border-b-0 align-top"
                    >
                      <td className="px-3 py-2 font-medium tabular-nums">
                        {it.item_key}
                      </td>
                      <td className="px-3 py-2 text-foreground-muted font-mono text-2xs">
                        {inputKind(it.input)}
                      </td>
                      <td className="px-3 py-2 text-foreground-muted">
                        {summariseInput(it.input)}
                      </td>
                      <td className="px-3 py-2 text-foreground-muted">
                        {it.expected !== undefined && it.expected !== null ? (
                          t("detail.expectedSet")
                        ) : (
                          <span className="text-foreground-subtle">—</span>
                        )}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </section>
      </PageStateView>
    </SettingsPageShell>
  );
}
