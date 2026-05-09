"use client";

import { useMemo, useState } from "react";
import { useTranslations } from "next-intl";
import { useQuery } from "@tanstack/react-query";

import { request } from "@/lib/api/client";
import { SkeletonList } from "@/components/ui/skeleton";
import { Heading } from "@/components/ui/heading";
import { CrossRefFlow } from "@/components/ontology/cross-ref-flow";

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

async function fetchMapSummary(): Promise<MapSummary> {
  return request<MapSummary>("/ontology/map-summary");
}

async function fetchAxisItems(kind: string): Promise<AxisItem[]> {
  const qs = new URLSearchParams({ kind });
  return request<AxisItem[]>(`/ontology/axis-items?${qs}`);
}

interface DrillTarget {
  axis: string;
  kind: string;
  count: number;
}

export default function OntologyMapPage() {
  const t = useTranslations("ontology.map");
  const [drill, setDrill] = useState<DrillTarget | null>(null);

  const { data, isLoading, error } = useQuery({
    queryKey: ["ontology-map-summary"],
    queryFn: () => fetchMapSummary(),
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
      <div className="p-6">
        <SkeletonList count={6} />
      </div>
    );
  }

  if (error || !data) {
    return (
      <div className="mx-auto mt-20 max-w-xl rounded border border-danger-border bg-danger-surface p-6 text-sm text-danger-foreground">
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
        <Heading level={1} size={3}>
          {t("title")}
        </Heading>
        <p className="mt-1 text-xs text-foreground-muted">
          {t("subtitle", {
            version: data.version ?? t("noVersion"),
            total: totalEntries,
          })}
        </p>
      </header>

      {data.danglers.length > 0 && (
        <aside className="mb-6 rounded border border-danger-border bg-danger-surface p-4">
          <Heading level={2} size={6} className="text-danger-foreground">
            {t("danglers.title", { count: data.danglers.length })}
          </Heading>
          <p className="mt-1 text-2xs text-danger-foreground">
            {t("danglers.hint")}
          </p>
          <ul className="mt-2 space-y-0.5 text-2xs">
            {data.danglers.slice(0, 10).map((d, idx) => (
              <li
                key={`${d.source_path}-${idx}`}
                className="font-mono text-danger-foreground"
              >
                {d.kind} → <span className="text-danger-foreground">{d.missing_id}</span>
                <span className="ms-2 text-foreground-muted">{d.source_path}</span>
              </li>
            ))}
            {data.danglers.length > 10 && (
              <li className="text-2xs italic text-foreground-muted">
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

      <CrossRefFlow />

      {drill && (
        <AxisDrillModal
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
    <article className="rounded-lg border border-divider bg-surface-base p-5">
      <header className="mb-3 flex items-baseline justify-between">
        <Heading level={3} size={6}>
          {title}
        </Heading>
        <span className="rounded bg-surface-inset px-1.5 py-0.5 text-2xs font-medium text-foreground-muted">
          {total}
        </span>
      </header>
      <p className="mb-3 text-2xs text-foreground-muted">{description}</p>
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
                "flex w-full items-center justify-between rounded px-1 py-0.5 text-start " +
                (disabled
                  ? "cursor-default text-foreground-muted"
                  : "cursor-pointer text-foreground hover:bg-surface-inset hover:text-concept-foreground-muted")
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
  axis,
  kind,
  count,
  onClose,
}: {
  axis: string;
  kind: string;
  count: number;
  onClose: () => void;
}) {
  const t = useTranslations("ontology.map");
  const tDrill = useTranslations("ontology.map.drill");
  const { data, isLoading, error } = useQuery({
    queryKey: ["ontology-axis-items", kind],
    queryFn: () => fetchAxisItems(kind),
  });

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-labelledby="axis-drill-title"
      className="fixed inset-0 z-modal flex items-center justify-center bg-surface-base/40 p-4 backdrop-blur-sm"
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div className="flex max-h-[80vh] w-full max-w-2xl flex-col rounded-lg border border-divider bg-surface-base shadow-4">
        <header className="flex items-baseline justify-between border-b border-divider px-5 py-3">
          <div>
            <h2
              id="axis-drill-title"
              className="text-sm font-semibold text-foreground-strong"
            >
              {t(`axes.${axis}.kinds.${kind}`)}
            </h2>
            <p className="mt-0.5 text-2xs text-foreground-muted">
              {tDrill("subtitle", { count })}
            </p>
          </div>
          <button
            type="button"
            onClick={onClose}
            aria-label={tDrill("close")}
            className="rounded px-2 py-0.5 text-xs text-foreground-muted hover:bg-surface-inset"
          >
            ✕
          </button>
        </header>

        <div className="min-h-0 flex-1 overflow-y-auto px-5 py-3 text-xs">
          {isLoading && (
            <p className="py-10 text-center text-foreground-muted">
              {tDrill("loading")}
            </p>
          )}
          {error && (
            <p className="py-10 text-center text-danger-foreground">
              {tDrill("loadError", {
                message:
                  error instanceof Error ? error.message : tDrill("unknownError"),
              })}
            </p>
          )}
          {!isLoading && !error && data && data.length === 0 && (
            <p className="py-10 text-center text-foreground-muted">
              {tDrill("empty")}
            </p>
          )}
          {!isLoading && !error && data && data.length > 0 && (
            <ul className="divide-y divide-divider-soft/60">
              {data.map((item) => (
                <li key={item.id} className="py-2">
                  <p className="font-medium text-foreground-strong">
                    {item.label}
                  </p>
                  {item.id !== item.label && (
                    <p className="mt-0.5 font-mono text-2xs text-foreground-muted">
                      {item.id}
                    </p>
                  )}
                  {item.description && (
                    <p className="mt-1 text-2xs text-foreground-muted">
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
