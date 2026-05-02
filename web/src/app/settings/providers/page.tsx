"use client";

import { useQuery } from "@tanstack/react-query";
import { useTranslations } from "next-intl";

import { getHealth, type HealthResponse } from "@/lib/api";
import { ErrorState } from "@/components/ui/error-state";
import { SkeletonCard } from "@/components/ui/skeleton";
import { StatusBadge, type StatusTone } from "@/components/ui/status-badge";
import { SettingsPageShell } from "@/components/layout/settings-page-shell";

type KnownOverallStatus = "ok" | "degraded" | "unavailable";
function isKnownOverall(s: string): s is KnownOverallStatus {
  return s === "ok" || s === "degraded" || s === "unavailable";
}

const providersKeys = {
  health: () => ["providers", "health"] as const,
};

export default function ProvidersPage() {
  const t = useTranslations("settings.providers");
  const tCommon = useTranslations("common");
  const query = useQuery({
    queryKey: providersKeys.health(),
    queryFn: () => getHealth(),
  });
  const health: HealthResponse | undefined = query.data;

  if (query.isLoading) {
    return (
      <SettingsPageShell title={t("title")}>
        <div className="space-y-4">
          <SkeletonCard />
          <SkeletonCard />
          <SkeletonCard />
        </div>
      </SettingsPageShell>
    );
  }

  if (query.isError || !health) {
    return (
      <SettingsPageShell title={t("title")}>
        <div className="py-12">
          <ErrorState
            title={tCommon("loadError.title")}
            description={tCommon("loadError.description")}
            onRetry={() => query.refetch()}
            retryLabel={tCommon("retry")}
          />
        </div>
      </SettingsPageShell>
    );
  }

  return (
    <SettingsPageShell title={t("title")}>
      <div className="space-y-6">
        {/* Overall Status */}
          <section className="rounded-lg border border-divider bg-surface-base">
            <div className="border-b border-divider-soft px-6 py-4">
              <div className="flex items-center justify-between">
                <h2 className="text-sm font-semibold text-foreground-strong">
                  {t("serviceStatus")}
                </h2>
                <ServiceStatusPill status={health.status} />
              </div>
              <p className="mt-0.5 text-xs text-foreground-muted">
                {health.service} v{health.version}
              </p>
            </div>
          </section>

          {/* LLM Provider */}
          <section className="rounded-lg border border-divider bg-surface-base">
            <div className="border-b border-divider-soft px-6 py-4">
              <h2 className="text-sm font-semibold text-foreground-strong">
                {t("llm.title")}
              </h2>
              <p className="mt-0.5 text-xs text-foreground-muted">
                {t("llm.description")}
              </p>
            </div>
            <div className="divide-y divide-divider-soft">
              <ProviderRow
                label={t("llm.provider")}
                value={health.components.llm.provider}
              />
              <ProviderRow
                label={t("llm.model")}
                value={health.components.llm.model}
              />
            </div>
          </section>

          {/* PostgreSQL */}
          <section className="rounded-lg border border-divider bg-surface-base">
            <div className="border-b border-divider-soft px-6 py-4">
              <div className="flex items-center justify-between">
                <div>
                  <h2 className="text-sm font-semibold text-foreground-strong">
                    {t("postgres.title")}
                  </h2>
                  <p className="mt-0.5 text-xs text-foreground-muted">
                    {t("postgres.description")}
                  </p>
                </div>
                <ComponentStatusPill status={health.components.postgres} />
              </div>
            </div>
          </section>

          {/* Graph Database */}
          <section className="rounded-lg border border-divider bg-surface-base">
            <div className="border-b border-divider-soft px-6 py-4">
              <div className="flex items-center justify-between">
                <div>
                  <h2 className="text-sm font-semibold text-foreground-strong">
                    {health.components.graph_backend && health.components.graph_backend !== "none"
                      ? health.components.graph_backend
                      : t("graph.title")}
                  </h2>
                  <p className="mt-0.5 text-xs text-foreground-muted">
                    {t("graph.description")}
                  </p>
                </div>
                <ComponentStatusPill status={health.components.neo4j} />
              </div>
            </div>
          </section>
      </div>
    </SettingsPageShell>
  );
}

function ServiceStatusPill({ status }: { status: string }) {
  const t = useTranslations("settings.providers.status");
  const tone: StatusTone =
    status === "ok" ? "success" : status === "degraded" ? "warning" : "danger";
  const label = isKnownOverall(status) ? t(status) : status;
  return (
    <StatusBadge tone={tone} size="md" className="gap-1.5">
      <span className="h-1.5 w-1.5 rounded-full bg-current" />
      {label}
    </StatusBadge>
  );
}

function ComponentStatusPill({ status }: { status: string }) {
  const t = useTranslations("settings.providers.status");
  const isOk = status === "ok";
  return (
    <StatusBadge tone={isOk ? "success" : "danger"} size="md" className="gap-1.5">
      <span className="h-1.5 w-1.5 rounded-full bg-current" />
      {isOk ? t("connected") : t("unavailable")}
    </StatusBadge>
  );
}

function ProviderRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-center justify-between px-6 py-3">
      <span className="text-sm text-foreground-muted">
        {label}
      </span>
      <span className="max-w-[320px] truncate text-right font-mono text-sm text-foreground-strong">
        {value}
      </span>
    </div>
  );
}
