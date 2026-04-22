"use client";

// Phase 4.2 — Complete Map dashboard. Renders six axis cards
// (Topology / Vocabulary / Registry / Strategy / VOL / Governance)
// with entry counts + a dangling-references callout surfacing the
// Phase 1.7 integrity check. Drill-downs and cross-ref visualisation
// are follow-ups; this page is the count dashboard the operator
// lands on.

import { use } from "react";
import { useTranslations } from "next-intl";
import { useQuery } from "@tanstack/react-query";

import { request } from "@/lib/api/client";
import { Spinner } from "@/components/ui/spinner";

interface AxisEntry {
  kind: string;
  count: number;
}

interface MapSummary {
  ontology_id: string;
  version: string | null;
  topology: { entries: AxisEntry[] };
  vocabulary: { entries: AxisEntry[] };
  registry: { entries: AxisEntry[] };
  strategy: { entries: AxisEntry[] };
  vol: { entries: AxisEntry[] };
  governance: { entries: AxisEntry[] };
  danglers: Array<{ kind: string; source_path: string; missing_id: string }>;
}

async function fetchMapSummary(id: string): Promise<MapSummary> {
  return request<MapSummary>(`/ontologies/${encodeURIComponent(id)}/map-summary`);
}

export default function OntologyMapPage({
  params,
}: {
  params: Promise<{ id: string }>;
}) {
  const { id } = use(params);
  const t = useTranslations("ontology.map");

  const { data, isLoading, error } = useQuery({
    queryKey: ["ontology-map-summary", id],
    queryFn: () => fetchMapSummary(id),
  });

  if (isLoading) {
    return (
      <div className="flex min-h-[50vh] items-center justify-center">
        <Spinner />
      </div>
    );
  }

  if (error || !data) {
    return (
      <div className="mx-auto mt-20 max-w-xl rounded border border-rose-200 bg-rose-50 p-6 text-sm text-rose-700 dark:border-rose-900 dark:bg-rose-950/30 dark:text-rose-300">
        {t("loadError", {
          message: error instanceof Error ? error.message : t("unknownError"),
        })}
      </div>
    );
  }

  const axes: Array<{ key: string; data: AxisEntry[] }> = [
    { key: "topology", data: data.topology.entries },
    { key: "vocabulary", data: data.vocabulary.entries },
    { key: "registry", data: data.registry.entries },
    { key: "strategy", data: data.strategy.entries },
    { key: "vol", data: data.vol.entries },
    { key: "governance", data: data.governance.entries },
  ];

  const totalEntries = axes.reduce(
    (sum, a) => sum + a.data.reduce((s, e) => s + e.count, 0),
    0,
  );

  return (
    <div className="mx-auto max-w-6xl px-6 py-8">
      <header className="mb-6">
        <h1 className="text-xl font-semibold text-zinc-900 dark:text-zinc-100">
          {t("title")}
        </h1>
        <p className="mt-1 text-xs text-muted-foreground">
          {t("subtitle", {
            version: data.version ?? t("noVersion"),
            total: totalEntries,
          })}
        </p>
      </header>

      {data.danglers.length > 0 && (
        <aside className="mb-6 rounded border border-rose-200 bg-rose-50 p-4 dark:border-rose-900 dark:bg-rose-950/30">
          <h2 className="text-xs font-semibold text-rose-700 dark:text-rose-300">
            {t("danglers.title", { count: data.danglers.length })}
          </h2>
          <p className="mt-1 text-[11px] text-rose-600 dark:text-rose-400">
            {t("danglers.hint")}
          </p>
          <ul className="mt-2 space-y-0.5 text-[11px]">
            {data.danglers.slice(0, 10).map((d, idx) => (
              <li
                key={`${d.source_path}-${idx}`}
                className="font-mono text-rose-700 dark:text-rose-300"
              >
                {d.kind} → <span className="text-rose-500">{d.missing_id}</span>
                <span className="ml-2 text-muted-foreground">{d.source_path}</span>
              </li>
            ))}
            {data.danglers.length > 10 && (
              <li className="text-[10px] italic text-muted-foreground">
                {t("danglers.more", { n: data.danglers.length - 10 })}
              </li>
            )}
          </ul>
        </aside>
      )}

      <div className="grid grid-cols-1 gap-4 md:grid-cols-2 lg:grid-cols-3">
        {axes.map(({ key, data: entries }) => (
          <AxisCard
            key={key}
            title={t(`axes.${key}.title`)}
            description={t(`axes.${key}.description`)}
            entries={entries}
            labelFor={(k) => t(`axes.${key}.kinds.${k}`)}
          />
        ))}
      </div>
    </div>
  );
}

function AxisCard({
  title,
  description,
  entries,
  labelFor,
}: {
  title: string;
  description: string;
  entries: AxisEntry[];
  labelFor: (kind: string) => string;
}) {
  const total = entries.reduce((s, e) => s + e.count, 0);
  return (
    <article className="rounded-lg border border-zinc-200 bg-white p-5 dark:border-zinc-800 dark:bg-zinc-950">
      <header className="mb-3 flex items-baseline justify-between">
        <h3 className="text-sm font-semibold text-zinc-900 dark:text-zinc-100">
          {title}
        </h3>
        <span className="rounded bg-zinc-100 px-1.5 py-0.5 text-[10px] font-medium text-zinc-600 dark:bg-zinc-800 dark:text-zinc-300">
          {total}
        </span>
      </header>
      <p className="mb-3 text-[11px] text-muted-foreground">{description}</p>
      <dl className="space-y-1 text-xs">
        {entries.map((e) => (
          <div
            key={e.kind}
            className="flex items-center justify-between text-zinc-700 dark:text-zinc-300"
          >
            <dt>{labelFor(e.kind)}</dt>
            <dd className="font-medium tabular-nums">{e.count}</dd>
          </div>
        ))}
      </dl>
    </article>
  );
}
