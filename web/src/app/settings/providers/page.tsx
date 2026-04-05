"use client";

import { useState, useEffect } from "react";
import { getHealth, type HealthResponse } from "@/lib/api";
import { Spinner } from "@/components/ui/spinner";

export default function ProvidersPage() {
  const [health, setHealth] = useState<HealthResponse | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    getHealth()
      .then(setHealth)
      .catch((err) => setError(err instanceof Error ? err.message : "Failed to load"))
      .finally(() => setLoading(false));
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
        Providers
      </h1>
      <p className="mt-1 text-sm text-zinc-500 dark:text-zinc-400">
        Provider configuration is managed via{" "}
        <code className="rounded bg-zinc-200 px-1 py-0.5 text-xs dark:bg-zinc-800">
          ontosyx.toml
        </code>{" "}
        and environment variables.
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
                  Service Status
                </h2>
                <StatusBadge status={health.status} />
              </div>
              <p className="mt-0.5 text-xs text-zinc-500 dark:text-zinc-400">
                {health.service} v{health.version}
              </p>
            </div>
          </section>

          {/* LLM Provider */}
          <section className="rounded-lg border border-zinc-200 bg-white dark:border-zinc-800 dark:bg-zinc-900">
            <div className="border-b border-zinc-100 px-6 py-4 dark:border-zinc-800">
              <h2 className="text-sm font-semibold text-zinc-900 dark:text-zinc-100">
                LLM Provider
              </h2>
              <p className="mt-0.5 text-xs text-zinc-500 dark:text-zinc-400">
                Language model used for ontology design, query translation, and
                explanations
              </p>
            </div>
            <div className="divide-y divide-zinc-100 dark:divide-zinc-800">
              <ProviderRow
                label="Provider"
                value={health.components.llm.provider}
              />
              <ProviderRow
                label="Model"
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
                    PostgreSQL
                  </h2>
                  <p className="mt-0.5 text-xs text-zinc-500 dark:text-zinc-400">
                    Application state database (projects, ontologies, query history)
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
                      : "Graph Database"}
                  </h2>
                  <p className="mt-0.5 text-xs text-zinc-500 dark:text-zinc-400">
                    Graph database for ontology queries and data exploration
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
  const styles =
    status === "ok"
      ? "bg-emerald-100 text-emerald-700 dark:bg-emerald-900/30 dark:text-emerald-400"
      : status === "degraded"
        ? "bg-amber-100 text-amber-700 dark:bg-amber-900/30 dark:text-amber-400"
        : "bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400";

  const label =
    status === "ok"
      ? "Healthy"
      : status === "degraded"
        ? "Degraded"
        : "Unavailable";

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
      {isOk ? "Connected" : "Unavailable"}
    </span>
  );
}

function ProviderRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-center justify-between px-6 py-3">
      <span className="text-sm text-zinc-500 dark:text-zinc-400">
        {label}
      </span>
      <span className="max-w-[320px] truncate text-right font-mono text-sm text-zinc-900 dark:text-zinc-100">
        {value}
      </span>
    </div>
  );
}
