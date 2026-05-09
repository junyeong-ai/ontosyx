"use client";

import { useQuery } from "@tanstack/react-query";
import { useTranslations } from "next-intl";
import type { ReactNode } from "react";

import { getHealth } from "@/lib/api";
import type { HealthResponse } from "@/types/api";
import { SkeletonCard } from "@/components/ui/skeleton";
import { Heading } from "@/components/ui/heading";
import { StatusBadge, type StatusTone } from "@/components/ui/status-badge";
import { SettingsPageShell } from "@/components/layout/settings-page-shell";
import { PageStateView } from "@/components/layout/page-state-view";
import type { PageState } from "@/components/layout/page-state";

type KnownOverallStatus = "ok" | "degraded" | "unavailable";
function isKnownOverall(s: string): s is KnownOverallStatus {
  return s === "ok" || s === "degraded" || s === "unavailable";
}

const providersKeys = {
  health: () => ["providers", "health"] as const,
};

export default function ProvidersPage() {
  const t = useTranslations("settings.runtime.providers");
  const tCommon = useTranslations("common");
  const query = useQuery({
    queryKey: providersKeys.health(),
    queryFn: () => getHealth(),
  });
  const health: HealthResponse | undefined = query.data;

  const pageState: PageState = query.isLoading
    ? { kind: "loading" }
    : query.isError || !health
      ? { kind: "error", onRetry: () => void query.refetch() }
      : { kind: "data" };

  return (
    <SettingsPageShell title={t("title")}>
      <PageStateView
        state={pageState}
        skeleton={
          <div className="space-y-4">
            <SkeletonCard />
            <SkeletonCard />
            <SkeletonCard />
          </div>
        }
        error={{
          title: tCommon("loadError.title"),
          description: tCommon("loadError.description"),
          retryLabel: tCommon("retry"),
        }}
      >
        {health && <ProvidersBody t={t} health={health} />}
      </PageStateView>
    </SettingsPageShell>
  );
}

function ProvidersBody({
  t,
  health,
}: {
  t: ReturnType<typeof useTranslations<"settings.runtime.providers">>;
  health: HealthResponse;
}) {
  return (
    <div className="space-y-6">
      <section className="rounded-lg border border-divider bg-surface-base">
        <div className="border-b border-divider-soft px-6 py-4">
          <div className="flex items-center justify-between">
            <Heading level={2} size={6}>
              {t("serviceStatus")}
            </Heading>
            <ServiceStatusPill status={health.status} />
          </div>
          <p className="mt-0.5 text-xs text-foreground-muted">
            {health.service} v{health.version}
          </p>
        </div>
      </section>

      <section className="rounded-lg border border-divider bg-surface-base">
        <div className="border-b border-divider-soft px-6 py-4">
          <Heading level={2} size={6}>
            {t("llm.title")}
          </Heading>
          <p className="mt-0.5 text-xs text-foreground-muted">
            {t("llm.description")}
          </p>
        </div>
        <div className="divide-y divide-divider-soft">
          <ProviderRow
            label={t("llm.status")}
            value={<LlmStatusPill status={health.components.llm.status} />}
          />
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

      <section className="rounded-lg border border-divider bg-surface-base">
        <div className="border-b border-divider-soft px-6 py-4">
          <div className="flex items-center justify-between">
            <div>
              <Heading level={2} size={6}>
                {t("postgres.title")}
              </Heading>
              <p className="mt-0.5 text-xs text-foreground-muted">
                {t("postgres.description")}
              </p>
            </div>
            <ComponentStatusPill status={health.components.postgres} />
          </div>
        </div>
      </section>

      <section className="rounded-lg border border-divider bg-surface-base">
        <div className="border-b border-divider-soft px-6 py-4">
          <div className="flex items-center justify-between">
            <div>
              <Heading level={2} size={6}>
                {health.components.graph_backend && health.components.graph_backend !== "none"
                  ? health.components.graph_backend
                  : t("graph.title")}
              </Heading>
              <p className="mt-0.5 text-xs text-foreground-muted">
                {t("graph.description")}
              </p>
            </div>
            <ComponentStatusPill status={health.components.graph} />
          </div>
        </div>
      </section>
    </div>
  );
}

function ServiceStatusPill({ status }: { status: string }) {
  const t = useTranslations("settings.runtime.providers.status");
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
  const t = useTranslations("settings.runtime.providers.status");
  const isOk = status === "ok";
  return (
    <StatusBadge tone={isOk ? "success" : "danger"} size="md" className="gap-1.5">
      <span className="h-1.5 w-1.5 rounded-full bg-current" />
      {isOk ? t("connected") : t("unavailable")}
    </StatusBadge>
  );
}

function LlmStatusPill({ status }: { status: string }) {
  const t = useTranslations("settings.runtime.providers.status");
  const tone: StatusTone =
    status === "configured" ? "warning" : status === "ok" ? "success" : "danger";
  const label = status === "configured" ? t("configured") : status;
  return (
    <StatusBadge tone={tone} size="md" className="gap-1.5">
      <span className="h-1.5 w-1.5 rounded-full bg-current" />
      {label}
    </StatusBadge>
  );
}

function ProviderRow({
  label,
  value,
}: {
  label: string;
  value: string | ReactNode;
}) {
  return (
    <div className="flex items-center justify-between px-6 py-3">
      <span className="text-sm text-foreground-muted">
        {label}
      </span>
      {typeof value === "string" ? (
        <span className="max-w-[320px] truncate text-end font-mono text-sm text-foreground-strong">
          {value}
        </span>
      ) : (
        value
      )}
    </div>
  );
}
