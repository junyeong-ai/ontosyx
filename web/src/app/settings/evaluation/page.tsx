"use client";

import { useTranslations } from "next-intl";
import Link from "next/link";

import { useAuth } from "@/hooks/use-auth";
import { useEvaluationRuns } from "@/hooks/api/use-evaluation";
import { SettingsPageShell } from "@/components/layout/settings-page-shell";
import { PageStateView } from "@/components/layout/page-state-view";
import type { PageState } from "@/components/layout/page-state";
import { EmptyState } from "@/components/ui/empty-state";
import { SkeletonList } from "@/components/ui/skeleton";
import { Heading } from "@/components/ui/heading";
import { cn } from "@/lib/cn";
import type { EvaluationRunStatus } from "@/types/evaluation";

/**
 * Status pill colour binding. Each status maps to a token-based
 * background + foreground pair so the dashboard's chrome-light
 * style stays in lockstep with the rest of the surface.
 */
const STATUS_TONE: Record<
  EvaluationRunStatus,
  { bg: string; fg: string; ring: string }
> = {
  running: {
    bg: "bg-info-surface",
    fg: "text-info-foreground",
    ring: "ring-info-border",
  },
  succeeded: {
    bg: "bg-success-surface",
    fg: "text-success-foreground",
    ring: "ring-success-border",
  },
  failed: {
    bg: "bg-danger-surface",
    fg: "text-danger-foreground",
    ring: "ring-danger-border",
  },
  cancelled: {
    bg: "bg-surface-inset",
    fg: "text-foreground-muted",
    ring: "ring-divider",
  },
};

function StatusPill({
  status,
  label,
}: {
  status: EvaluationRunStatus;
  label: string;
}) {
  const tone = STATUS_TONE[status];
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

function formatTimestamp(value: string) {
  const d = new Date(value);
  if (Number.isNaN(d.getTime())) return value;
  return d.toLocaleString();
}

export default function EvaluationPage() {
  const t = useTranslations("settings.evaluation");
  const tCommon = useTranslations("common");
  const { isAdmin } = useAuth();
  const query = useEvaluationRuns();

  if (!isAdmin) {
    return (
      <SettingsPageShell title={t("title")} subtitle={t("description")}>
        <EmptyState title={t("adminOnly")} />
      </SettingsPageShell>
    );
  }

  const pageState: PageState = query.isLoading
    ? { kind: "loading" }
    : query.isError
      ? { kind: "error", onRetry: () => void query.refetch() }
      : (query.data?.items.length ?? 0) === 0
        ? { kind: "empty" }
        : { kind: "data" };

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
        empty={{
          title: t("emptyTitle"),
          description: t("emptyDescription"),
        }}
      >
        <div className="overflow-auto rounded-xl border border-divider">
          <table className="w-full text-sm">
            <thead className="bg-surface-inset text-2xs font-medium uppercase tracking-wide text-foreground-muted">
              <tr>
                <th className="px-4 py-2 text-left">{t("table.name")}</th>
                <th className="px-4 py-2 text-left">{t("table.status")}</th>
                <th className="px-4 py-2 text-left">{t("table.startedAt")}</th>
                <th className="px-4 py-2 text-left">{t("table.completedAt")}</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-divider">
              {(query.data?.items ?? []).map((run) => (
                <tr key={run.id} className="hover:bg-surface-inset">
                  <td className="px-4 py-2">
                    <Link
                      href={`/settings/evaluation/${run.id}`}
                      className="block hover:underline"
                    >
                      <Heading level={3} size={6}>
                        {run.name}
                      </Heading>
                      {run.description ? (
                        <p className="mt-0.5 text-xs text-foreground-muted">
                          {run.description}
                        </p>
                      ) : null}
                    </Link>
                  </td>
                  <td className="px-4 py-2">
                    <StatusPill
                      status={run.status}
                      label={t(`status.${run.status}`)}
                    />
                  </td>
                  <td className="px-4 py-2 text-foreground-muted tabular-nums">
                    {formatTimestamp(run.started_at)}
                  </td>
                  <td className="px-4 py-2 text-foreground-muted tabular-nums">
                    {run.completed_at ? formatTimestamp(run.completed_at) : "—"}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </PageStateView>
    </SettingsPageShell>
  );
}
