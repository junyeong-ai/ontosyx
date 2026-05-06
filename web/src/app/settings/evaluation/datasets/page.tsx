"use client";

import Link from "next/link";
import { useTranslations } from "next-intl";
import { useState } from "react";

import { useAuth } from "@/hooks/use-auth";
import {
  useDeleteEvaluationDataset,
  useEvaluationDatasets,
  useUpsertEvaluationDataset,
} from "@/hooks/api/use-evaluation";
import { useConfirm } from "@/components/providers/confirm-provider";
import { SettingsPageShell } from "@/components/layout/settings-page-shell";
import { PageStateView } from "@/components/layout/page-state-view";
import type { PageState } from "@/components/layout/page-state";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { SkeletonList } from "@/components/ui/skeleton";
import { Heading } from "@/components/ui/heading";
import { SettingsInput } from "@/components/ui/form-input";
import { toast } from "@/components/ui/toast";

/// Dataset list page. Datasets are workspace-scoped, name-keyed
/// via UPSERT — re-importing under the same name preserves
/// `id` + `created_at` and updates `description` only. The page
/// gives operators a place to see what datasets exist (so the
/// promote-to-dataset prompt has a recognisable id to paste)
/// and a small inline form to declare a new dataset header
/// without reaching for a curl command.
export default function EvaluationDatasetsPage() {
  const t = useTranslations("settings.evaluation.datasets");
  const tCommon = useTranslations("common");
  const { isAdmin } = useAuth();
  const datasetsQuery = useEvaluationDatasets();
  const upsert = useUpsertEvaluationDataset();
  const remove = useDeleteEvaluationDataset();
  const confirm = useConfirm();
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");

  if (!isAdmin) {
    return (
      <SettingsPageShell title={t("title")} subtitle={t("description")}>
        <EmptyState title={t("adminOnly")} />
      </SettingsPageShell>
    );
  }

  const pageState: PageState = datasetsQuery.isLoading
    ? { kind: "loading" }
    : datasetsQuery.isError
      ? { kind: "error", onRetry: () => void datasetsQuery.refetch() }
      : { kind: "data" };

  const onCreate = () => {
    const trimmed = name.trim();
    if (trimmed.length === 0) return;
    upsert.mutate(
      { name: trimmed, description: description.trim() },
      {
        onSuccess: (dataset) => {
          toast.success(
            t("create.successToast", { name: dataset.name }),
          );
          setName("");
          setDescription("");
        },
        onError: (err) => {
          toast.error(
            t("create.errorToast", {
              error: err instanceof Error ? err.message : String(err),
            }),
          );
        },
      },
    );
  };
  const canCreate = name.trim().length > 0 && !upsert.isPending;

  const onDelete = async (id: string, datasetName: string) => {
    const ok = await confirm({
      title: t("delete.confirmTitle", { name: datasetName }),
      description: t("delete.confirmDescription"),
      confirmLabel: tCommon("delete"),
      variant: "danger",
    });
    if (!ok) return;
    remove.mutate(id, {
      onSuccess: () => toast.success(t("delete.successToast")),
      onError: (err) => {
        toast.error(
          t("delete.errorToast", {
            error: err instanceof Error ? err.message : String(err),
          }),
        );
      },
    });
  };

  const datasets = datasetsQuery.data?.items ?? [];

  return (
    <SettingsPageShell title={t("title")} subtitle={t("description")}>
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
            {t("create.title")}
          </Heading>
          <p className="mt-1 text-xs text-foreground-muted">
            {t("create.description")}
          </p>
          <div className="mt-3 grid grid-cols-1 gap-3 md:grid-cols-[1fr_2fr_auto] md:items-end">
            <SettingsInput
              label={t("create.nameLabel")}
              placeholder={t("create.namePlaceholder")}
              value={name}
              onChange={(e) => setName(e.target.value)}
            />
            <SettingsInput
              label={t("create.descriptionLabel")}
              placeholder={t("create.descriptionPlaceholder")}
              value={description}
              onChange={(e) => setDescription(e.target.value)}
            />
            <Button
              type="button"
              onClick={onCreate}
              disabled={!canCreate}
              loading={upsert.isPending}
            >
              {upsert.isPending
                ? t("create.submitting")
                : t("create.submit")}
            </Button>
          </div>
        </section>

        <section className="rounded-xl border border-divider bg-surface-base p-4">
          <header className="mb-3 flex items-baseline justify-between">
            <Heading level={2} size={5}>
              {t("listTitle")}
            </Heading>
            <span className="text-2xs text-foreground-muted tabular-nums">
              {datasets.length}
            </span>
          </header>
          {datasets.length === 0 ? (
            <EmptyState title={t("emptyTitle")} description={t("emptyDescription")} />
          ) : (
            <div className="overflow-x-auto">
              <table className="w-full text-sm">
                <thead>
                  <tr className="border-b border-divider text-start text-2xs font-medium uppercase tracking-wide text-foreground-muted">
                    <th className="px-3 py-2 text-start">{t("col.name")}</th>
                    <th className="px-3 py-2 text-start">{t("col.description")}</th>
                    <th className="px-3 py-2 text-start">{t("col.id")}</th>
                    <th className="px-3 py-2 text-end">{t("col.actions")}</th>
                  </tr>
                </thead>
                <tbody>
                  {datasets.map((d) => (
                    <tr key={d.id} className="border-b border-divider last:border-b-0">
                      <td className="px-3 py-2 font-medium">
                        <Link
                          href={`/settings/evaluation/datasets/${encodeURIComponent(d.id)}`}
                          className="text-brand-foreground hover:underline"
                        >
                          {d.name}
                        </Link>
                      </td>
                      <td className="px-3 py-2 text-foreground-muted">
                        {d.description || (
                          <span className="text-foreground-subtle">—</span>
                        )}
                      </td>
                      <td className="px-3 py-2 font-mono text-2xs text-foreground-muted">
                        {d.id}
                      </td>
                      <td className="px-3 py-2 text-end">
                        <Button
                          type="button"
                          variant="outline"
                          size="sm"
                          onClick={() => onDelete(d.id, d.name)}
                          disabled={remove.isPending}
                        >
                          {tCommon("delete")}
                        </Button>
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
