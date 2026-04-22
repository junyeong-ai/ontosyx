"use client";

// Phase 4.2 — Complete Map dashboard. Renders six axis cards
// (Topology / Vocabulary / Registry / Strategy / VOL / Governance)
// with entry counts + a dangling-references callout surfacing the
// Phase 1.7 integrity check.
//
// Phase 4.2 follow-up: count rows are now clickable. Clicking opens
// an in-page drill-down modal that fetches
// `GET /api/ontologies/{id}/axis-items?kind=<kind>` and renders
// the matching entries ({id, label, description}).

import { useMemo, useState, use } from "react";
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

interface AxisItem {
  id: string;
  label: string;
  description: string;
}

async function fetchMapSummary(id: string): Promise<MapSummary> {
  return request<MapSummary>(`/ontologies/${encodeURIComponent(id)}/map-summary`);
}

async function fetchAxisItems(
  ontologyId: string,
  kind: string,
): Promise<AxisItem[]> {
  const qs = new URLSearchParams({ kind });
  return request<AxisItem[]>(
    `/ontologies/${encodeURIComponent(ontologyId)}/axis-items?${qs}`,
  );
}

interface DrillTarget {
  axis: string;
  kind: string;
  count: number;
}

export default function OntologyMapPage({
  params,
}: {
  params: Promise<{ id: string }>;
}) {
  const { id } = use(params);
  const t = useTranslations("ontology.map");
  const [drill, setDrill] = useState<DrillTarget | null>(null);

  const { data, isLoading, error } = useQuery({
    queryKey: ["ontology-map-summary", id],
    queryFn: () => fetchMapSummary(id),
  });

  const axes = useMemo(() => {
    if (!data) return [];
    return [
      { key: "topology", data: data.topology.entries },
      { key: "vocabulary", data: data.vocabulary.entries },
      { key: "registry", data: data.registry.entries },
      { key: "strategy", data: data.strategy.entries },
      { key: "vol", data: data.vol.entries },
      { key: "governance", data: data.governance.entries },
    ];
  }, [data]);

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
            onDrill={(entry) =>
              setDrill({ axis: key, kind: entry.kind, count: entry.count })
            }
          />
        ))}
      </div>

      {drill && (
        <AxisDrillModal
          ontologyId={id}
          axis={drill.axis}
          kind={drill.kind}
          count={drill.count}
          onClose={() => setDrill(null)}
        />
      )}
    </div>
  );
}

function AxisCard({
  title,
  description,
  entries,
  labelFor,
  onDrill,
}: {
  title: string;
  description: string;
  entries: AxisEntry[];
  labelFor: (kind: string) => string;
  onDrill: (entry: AxisEntry) => void;
}) {
  const t = useTranslations("ontology.map.drill");
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
      <dl className="space-y-0.5 text-xs">
        {entries.map((e) => {
          const disabled = e.count === 0;
          return (
            <button
              key={e.kind}
              type="button"
              disabled={disabled}
              onClick={() => onDrill(e)}
              aria-label={t("openAria", {
                kind: labelFor(e.kind),
                count: e.count,
              })}
              className={
                "flex w-full items-center justify-between rounded px-1 py-0.5 text-left " +
                (disabled
                  ? "cursor-default text-muted-foreground"
                  : "cursor-pointer text-zinc-700 hover:bg-zinc-100 hover:text-violet-700 dark:text-zinc-300 dark:hover:bg-zinc-800/60 dark:hover:text-violet-400")
              }
            >
              <dt>{labelFor(e.kind)}</dt>
              <dd className="font-medium tabular-nums">{e.count}</dd>
            </button>
          );
        })}
      </dl>
    </article>
  );
}

function AxisDrillModal({
  ontologyId,
  axis,
  kind,
  count,
  onClose,
}: {
  ontologyId: string;
  axis: string;
  kind: string;
  count: number;
  onClose: () => void;
}) {
  const t = useTranslations("ontology.map");
  const tDrill = useTranslations("ontology.map.drill");
  const { data, isLoading, error } = useQuery({
    queryKey: ["ontology-axis-items", ontologyId, kind],
    queryFn: () => fetchAxisItems(ontologyId, kind),
  });

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-labelledby="axis-drill-title"
      className="fixed inset-0 z-50 flex items-center justify-center bg-zinc-900/40 p-4 backdrop-blur-sm"
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div className="flex max-h-[80vh] w-full max-w-2xl flex-col rounded-lg border border-zinc-200 bg-white shadow-xl dark:border-zinc-700 dark:bg-zinc-950">
        <header className="flex items-baseline justify-between border-b border-zinc-200 px-5 py-3 dark:border-zinc-800">
          <div>
            <h2
              id="axis-drill-title"
              className="text-sm font-semibold text-zinc-900 dark:text-zinc-100"
            >
              {t(`axes.${axis}.kinds.${kind}`)}
            </h2>
            <p className="mt-0.5 text-[11px] text-muted-foreground">
              {tDrill("subtitle", { count })}
            </p>
          </div>
          <button
            type="button"
            onClick={onClose}
            aria-label={tDrill("close")}
            className="rounded px-2 py-0.5 text-xs text-muted-foreground hover:bg-zinc-100 dark:hover:bg-zinc-800"
          >
            ✕
          </button>
        </header>

        <div className="min-h-0 flex-1 overflow-y-auto px-5 py-3 text-xs">
          {isLoading && (
            <p className="py-10 text-center text-muted-foreground">
              {tDrill("loading")}
            </p>
          )}
          {error && (
            <p className="py-10 text-center text-rose-600 dark:text-rose-400">
              {tDrill("loadError", {
                message:
                  error instanceof Error ? error.message : tDrill("unknownError"),
              })}
            </p>
          )}
          {!isLoading && !error && data && data.length === 0 && (
            <p className="py-10 text-center text-muted-foreground">
              {tDrill("empty")}
            </p>
          )}
          {!isLoading && !error && data && data.length > 0 && (
            <ul className="divide-y divide-zinc-100 dark:divide-zinc-800/60">
              {data.map((item) => (
                <li key={item.id} className="py-2">
                  <p className="font-medium text-zinc-900 dark:text-zinc-100">
                    {item.label}
                  </p>
                  {item.id !== item.label && (
                    <p className="mt-0.5 font-mono text-[10px] text-muted-foreground">
                      {item.id}
                    </p>
                  )}
                  {item.description && (
                    <p className="mt-1 text-[11px] text-zinc-600 dark:text-zinc-400">
                      {item.description}
                    </p>
                  )}
                </li>
              ))}
            </ul>
          )}
        </div>
      </div>
    </div>
  );
}
