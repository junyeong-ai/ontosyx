"use client";

import { useState, useEffect } from "react";
import { useTranslations } from "next-intl";
import { getHealth, type HealthResponse } from "@/lib/api";
import { Spinner } from "@/components/ui/spinner";

type KnownOverallStatus = "ok" | "degraded" | "unavailable";
function isKnownOverall(s: string): s is KnownOverallStatus {
  return s === "ok" || s === "degraded" || s === "unavailable";
}

export default function ProvidersPage() {
  const t = useTranslations("settings.providers");
  const [health, setHealth] = useState<HealthResponse | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    getHealth()
      .then(setHealth)
      .catch((err) => setError(err instanceof Error ? err.message : t("loadError")))
      .finally(() => setLoading(false));
    // t is stable across renders (it's memoised by next-intl) so omitting
    // it from deps is fine; adding it causes an infinite reload on some
    // i18n hot-reload setups.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  if (loading) {
    return (
      <div className="flex items-center justify-center py-20">
        <Spinner size="lg" className="text-emerald-500" />
      </div>
    );
  }

  return (
    <div>
      <h1 className="text-xl font-semibold text-zinc-900 dark:text-zinc-100">
        {t("title")}
      </h1>
      <p className="mt-1 text-sm text-zinc-500 dark:text-muted-foreground">
        {t.rich("description", {
          configFile: () => (
            <code className="rounded bg-zinc-200 px-1 py-0.5 text-xs dark:bg-zinc-800">
              ontosyx.toml
            </code>
          ),
        })}
      </p>

      {error && (
        <div className="mt-6 rounded-lg border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700 dark:border-red-800 dark:bg-red-950/30 dark:text-red-300">
          {error}
        </div>
      )}

      {health && (
        <div className="mt-6 space-y-6">
          {/* Overall Status */}
          <section className="rounded-lg border border-zinc-200 bg-white dark:border-zinc-800 dark:bg-zinc-900">
            <div className="border-b border-zinc-100 px-6 py-4 dark:border-zinc-800">
              <div className="flex items-center justify-between">
                <h2 className="text-sm font-semibold text-zinc-900 dark:text-zinc-100">
                  {t("serviceStatus")}
                </h2>
                <StatusBadge status={health.status} />
              </div>
              <p className="mt-0.5 text-xs text-zinc-500 dark:text-muted-foreground">
                {health.service} v{health.version}
              </p>
            </div>
          </section>

          {/* LLM Provider */}
          <section className="rounded-lg border border-zinc-200 bg-white dark:border-zinc-800 dark:bg-zinc-900">
            <div className="border-b border-zinc-100 px-6 py-4 dark:border-zinc-800">
              <h2 className="text-sm font-semibold text-zinc-900 dark:text-zinc-100">
                {t("llm.title")}
              </h2>
              <p className="mt-0.5 text-xs text-zinc-500 dark:text-muted-foreground">
                {t("llm.description")}
              </p>
            </div>
            <div className="divide-y divide-zinc-100 dark:divide-zinc-800">
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
          <section className="rounded-lg border border-zinc-200 bg-white dark:border-zinc-800 dark:bg-zinc-900">
            <div className="border-b border-zinc-100 px-6 py-4 dark:border-zinc-800">
              <div className="flex items-center justify-between">
                <div>
                  <h2 className="text-sm font-semibold text-zinc-900 dark:text-zinc-100">
                    {t("postgres.title")}
                  </h2>
                  <p className="mt-0.5 text-xs text-zinc-500 dark:text-muted-foreground">
                    {t("postgres.description")}
                  </p>
                </div>
                <ComponentBadge status={health.components.postgres} />
              </div>
            </div>
          </section>

          {/* Graph Database */}
          <section className="rounded-lg border border-zinc-200 bg-white dark:border-zinc-800 dark:bg-zinc-900">
            <div className="border-b border-zinc-100 px-6 py-4 dark:border-zinc-800">
              <div className="flex items-center justify-between">
                <div>
                  <h2 className="text-sm font-semibold text-zinc-900 dark:text-zinc-100">
                    {health.components.graph_backend && health.components.graph_backend !== "none"
                      ? health.components.graph_backend
                      : t("graph.title")}
                  </h2>
                  <p className="mt-0.5 text-xs text-zinc-500 dark:text-muted-foreground">
                    {t("graph.description")}
                  </p>
                </div>
                <ComponentBadge status={health.components.neo4j} />
              </div>
            </div>
          </section>
        </div>
      )}
    </div>
  );
}

function StatusBadge({ status }: { status: string }) {
  const t = useTranslations("settings.providers.status");
  const styles =
    status === "ok"
      ? "bg-emerald-100 text-emerald-700 dark:bg-emerald-900/30 dark:text-emerald-400"
      : status === "degraded"
        ? "bg-amber-100 text-amber-700 dark:bg-amber-900/30 dark:text-amber-400"
        : "bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400";

  // Translated label for the three statuses the service reports; any
  // other value falls back to the raw string so we still surface the
  // backend's state rather than a blank pill.
  const label = isKnownOverall(status) ? t(status) : status;

  return (
    <span
      className={`inline-flex items-center gap-1.5 rounded-full px-2.5 py-0.5 text-xs font-medium ${styles}`}
    >
      <span
        className={`h-1.5 w-1.5 rounded-full ${
          status === "ok"
            ? "bg-emerald-500"
            : status === "degraded"
              ? "bg-amber-500"
              : "bg-red-500"
        }`}
      />
      {label}
    </span>
  );
}

function ComponentBadge({ status }: { status: string }) {
  const t = useTranslations("settings.providers.status");
  const isOk = status === "ok";
  return (
    <span
      className={`inline-flex items-center gap-1.5 rounded-full px-2.5 py-0.5 text-xs font-medium ${
        isOk
          ? "bg-emerald-100 text-emerald-700 dark:bg-emerald-900/30 dark:text-emerald-400"
          : "bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400"
      }`}
    >
      <span
        className={`h-1.5 w-1.5 rounded-full ${
          isOk ? "bg-emerald-500" : "bg-red-500"
        }`}
      />
      {isOk ? t("connected") : t("unavailable")}
    </span>
  );
}

function ProviderRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-center justify-between px-6 py-3">
      <span className="text-sm text-zinc-500 dark:text-muted-foreground">
        {label}
      </span>
      <span className="max-w-[320px] truncate text-right font-mono text-sm text-zinc-900 dark:text-zinc-100">
        {value}
      </span>
    </div>
  );
}
