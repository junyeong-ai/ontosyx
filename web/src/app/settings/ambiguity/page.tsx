"use client";

import { useMemo, useState } from "react";
import { useTranslations } from "next-intl";
import { toast } from "sonner";

import { SettingsPageShell } from "@/components/layout/settings-page-shell";
import { ErrorState } from "@/components/ui/error-state";
import { SkeletonTable } from "@/components/ui/skeleton";
import {
  useAmbiguities,
  useResolveAmbiguity,
  useRevokeAmbiguity,
} from "@/hooks/api/use-ambiguities";
import type {
  AmbiguityMapping,
  AmbiguitySummary,
} from "@/lib/api/ambiguity";
import { ResolutionModal } from "@/components/ambiguity/resolution-modal";

type TabKey = "pending" | "resolved" | "stale";

function classify(summary: AmbiguitySummary): TabKey {
  if (!summary.active_resolution) return "pending";
  if (summary.active_resolution.context_source_hash !== summary.context.detection_source_hash) {
    return "stale";
  }
  return "resolved";
}

export default function AmbiguityAdminPage() {
  const t = useTranslations("settings.ambiguity");
  const tCommon = useTranslations("common");
  const [tab, setTab] = useState<TabKey>("pending");
  const [editing, setEditing] = useState<AmbiguitySummary | null>(null);

  const ambiguitiesQuery = useAmbiguities();
  const { data, isLoading, isError, refetch } = ambiguitiesQuery;
  const resolve = useResolveAmbiguity({
    onSuccess: () => {
      toast.success(t("toast.resolved"));
      setEditing(null);
    },
    onError: (err) => toast.error(err instanceof Error ? err.message : t("toast.resolveFailed")),
  });
  const revoke = useRevokeAmbiguity({
    onSuccess: () => toast.success(t("toast.revoked")),
    onError: (err) => toast.error(err instanceof Error ? err.message : t("toast.revokeFailed")),
  });

  const grouped = useMemo(() => {
    const items = data?.items ?? [];
    const by: Record<TabKey, AmbiguitySummary[]> = {
      pending: [],
      stale: [],
      resolved: [],
    };
    for (const item of items) {
      by[classify(item)].push(item);
    }
    return by;
  }, [data]);

  const activeList = grouped[tab];

  return (
    <SettingsPageShell title={t("title")} subtitle={t("subtitle")}>
      <div className="flex flex-col gap-4">

      <nav aria-label={t("tabsLabel")} className="flex gap-1 border-b border-divider">
        {(["pending", "stale", "resolved"] as const).map((k) => (
          <button
            key={k}
            type="button"
            onClick={() => setTab(k)}
            aria-pressed={tab === k}
            className={`relative px-3 py-2 text-xs font-medium ${
              tab === k
                ? "text-brand-foreground"
                : "text-muted-foreground hover:text-foreground dark:hover:text-foreground-muted"
            }`}
          >
            {t(`tabs.${k}`)}
            <span className="ml-1 rounded bg-surface-inset px-1 text-2xs">
              {grouped[k].length}
            </span>
            {tab === k && (
              <span className="absolute inset-x-0 -bottom-px h-0.5 bg-brand-solid" />
            )}
          </button>
        ))}
      </nav>

      {isLoading && <SkeletonTable rows={4} cols={6} />}

      {isError && (
        <ErrorState
          title={tCommon("loadError.title")}
          description={tCommon("loadError.description")}
          onRetry={() => refetch()}
          retryLabel={tCommon("retry")}
        />
      )}

      {!isLoading && !isError && activeList.length === 0 && (
        <p className="py-6 text-center text-xs text-muted-foreground">
          {t(`empty.${tab}`)}
        </p>
      )}

      {!isLoading && !isError && activeList.length > 0 && (
        <table className="w-full border-collapse text-xs">
          <thead>
            <tr className="border-b border-divider text-left text-2xs uppercase tracking-wider text-muted-foreground">
              <th className="py-2 pr-4 font-medium">{t("columns.source")}</th>
              <th className="py-2 pr-4 font-medium">{t("columns.column")}</th>
              <th className="py-2 pr-4 font-medium">{t("columns.kind")}</th>
              <th className="py-2 pr-4 font-medium">{t("columns.mapping")}</th>
              <th className="py-2 pr-4 font-medium">{t("columns.detected")}</th>
              <th className="py-2 pr-4 text-right font-medium">
                {t("columns.actions")}
              </th>
            </tr>
          </thead>
          <tbody>
            {activeList.map((row) => (
              <tr
                key={row.context.id}
                className="border-b border-divider-soft hover:bg-surface-raised"
              >
                <td className="py-2 pr-4 font-mono">{row.context.source_id}</td>
                <td className="py-2 pr-4">
                  <span className="text-muted-foreground">
                    {row.context.column.relation}.
                  </span>
                  <span className="font-medium">{row.context.column.column}</span>
                </td>
                <td className="py-2 pr-4">
                  <KindBadge kind={row.context.kind.kind} />
                </td>
                <td className="py-2 pr-4">
                  {row.active_resolution ? (
                    <MappingBadge mapping={row.active_resolution.mapping} />
                  ) : (
                    <span className="text-muted-foreground">—</span>
                  )}
                </td>
                <td className="py-2 pr-4 text-muted-foreground">
                  {new Date(row.context.detected_at).toLocaleDateString()}
                </td>
                <td className="py-2 pr-4 text-right">
                  <button
                    type="button"
                    onClick={() => setEditing(row)}
                    className="rounded px-2 py-1 text-2xs font-medium text-concept-foreground hover:bg-concept-surface"
                  >
                    {row.active_resolution ? t("actions.edit") : t("actions.resolve")}
                  </button>
                  {row.active_resolution && (
                    <button
                      type="button"
                      onClick={() => revoke.mutate(row.context.id)}
                      disabled={revoke.isPending}
                      className="ml-1 rounded px-2 py-1 text-2xs font-medium text-danger-foreground hover:bg-danger-surface disabled:opacity-50"
                    >
                      {t("actions.revoke")}
                    </button>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}

      {editing && (
        <ResolutionModal
          context={editing.context}
          active={editing.active_resolution}
          busy={resolve.isPending}
          onCancel={() => setEditing(null)}
          onSubmit={(mapping: AmbiguityMapping) => {
            resolve.mutate({ id: editing.context.id, mapping });
          }}
        />
      )}
      </div>
    </SettingsPageShell>
  );
}

function KindBadge({ kind }: { kind: "numeric_code" | "opaque_short_code" | "overloaded_name" }) {
  const t = useTranslations("settings.ambiguity.kind");
  const classes =
    kind === "numeric_code"
      ? "bg-info-surface text-info-foreground"
      : kind === "opaque_short_code"
        ? "bg-warning-surface text-warning-foreground"
        : "bg-danger-surface text-danger-foreground";
  return (
    <span className={`inline-flex rounded px-1.5 py-0.5 text-2xs ${classes}`}>
      {t(kind)}
    </span>
  );
}

function MappingBadge({ mapping }: { mapping: AmbiguityMapping }) {
  const t = useTranslations("settings.ambiguity.mappingBadge");
  if (mapping.kind === "value_map") {
    return (
      <span className="text-muted-foreground">
        {t("valueMap", { count: mapping.entries.length })}
      </span>
    );
  }
  if (mapping.kind === "code_system_ref") {
    return <span className="font-mono text-2xs">CS: {mapping.code_system_id}</span>;
  }
  return <span className="font-mono text-2xs">G: {mapping.term_id}</span>;
}
