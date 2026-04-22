"use client";

// Phase 4.4 — ExploreCanvas facet sidebar.
//
// Left-hand overlay on the explore layout showing type-level
// instance counts + active selection. Filter selections feed the
// existing search/expand flow; the "Save as segment" button
// emits an onSaveSegment callback that the parent wires into the
// Phase 3 SegmentDef API when an ontology id is available.
//
// Multi-hop depth control lives here so the canvas click handler
// reads the shared depth value. A Cmd/Ctrl-click elsewhere in the
// layout switches to a 3-hop expansion for one action (the layout
// wires this, not the sidebar itself).

import { useTranslations } from "next-intl";
import { useMemo } from "react";

import type { GraphOverview } from "@/lib/api/queries";

export interface ExploreFacetProps {
  overview: GraphOverview | null;
  loading: boolean;
  selectedLabels: string[];
  onToggleLabel: (label: string) => void;
  onClearLabels: () => void;
  expandDepth: 1 | 2 | 3;
  onChangeDepth: (depth: 1 | 2 | 3) => void;
  onSaveSegment?: () => void;
}

export function ExploreFacetSidebar({
  overview,
  loading,
  selectedLabels,
  onToggleLabel,
  onClearLabels,
  expandDepth,
  onChangeDepth,
  onSaveSegment,
}: ExploreFacetProps) {
  const t = useTranslations("workbench.explore.facet");
  const selectedSet = useMemo(
    () => new Set(selectedLabels),
    [selectedLabels],
  );

  const depths: Array<1 | 2 | 3> = [1, 2, 3];

  return (
    <aside
      aria-label={t("title")}
      className="flex h-full w-56 shrink-0 flex-col gap-4 border-r border-zinc-200 bg-white p-3 dark:border-zinc-800 dark:bg-zinc-950"
    >
      <section>
        <h2 className="mb-1 text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
          {t("depth.label")}
        </h2>
        <div
          role="radiogroup"
          aria-label={t("depth.label")}
          className="flex gap-1"
        >
          {depths.map((d) => (
            <button
              key={d}
              type="button"
              role="radio"
              aria-checked={d === expandDepth}
              onClick={() => onChangeDepth(d)}
              className={`flex-1 rounded border px-2 py-1 text-[11px] font-medium ${
                d === expandDepth
                  ? "border-violet-500 bg-violet-50 text-violet-700 dark:bg-violet-950/40 dark:text-violet-300"
                  : "border-zinc-200 bg-white text-muted-foreground hover:bg-zinc-50 dark:border-zinc-700 dark:bg-zinc-900 dark:hover:bg-zinc-800"
              }`}
            >
              {t("depth.hops", { n: d })}
            </button>
          ))}
        </div>
        <p className="mt-1 text-[10px] text-muted-foreground">
          {t("depth.cmdHint")}
        </p>
      </section>

      <section className="flex min-h-0 flex-1 flex-col">
        <div className="mb-1 flex items-center justify-between">
          <h2 className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
            {t("types.label")}
          </h2>
          {selectedLabels.length > 0 && (
            <button
              type="button"
              onClick={onClearLabels}
              className="text-[10px] text-violet-600 hover:underline dark:text-violet-400"
            >
              {t("types.clear")}
            </button>
          )}
        </div>

        {loading && (
          <p className="py-2 text-[11px] text-muted-foreground">
            {t("loading")}
          </p>
        )}
        {!loading && overview && overview.labels.length === 0 && (
          <p className="py-2 text-[11px] text-muted-foreground">
            {t("types.empty")}
          </p>
        )}
        {!loading && overview && (
          <ul className="flex flex-1 flex-col gap-0.5 overflow-auto pr-1 text-[11px]">
            {overview.labels.map((l) => {
              const selected = selectedSet.has(l.label);
              return (
                <li key={l.label}>
                  <button
                    type="button"
                    onClick={() => onToggleLabel(l.label)}
                    aria-pressed={selected}
                    className={`flex w-full items-center justify-between rounded px-2 py-1 ${
                      selected
                        ? "bg-violet-50 text-violet-700 dark:bg-violet-950/40 dark:text-violet-300"
                        : "hover:bg-zinc-100 dark:hover:bg-zinc-800"
                    }`}
                  >
                    <span className="truncate">{l.label}</span>
                    <span className="shrink-0 font-mono text-[10px] tabular-nums text-muted-foreground">
                      {l.count.toLocaleString()}
                    </span>
                  </button>
                </li>
              );
            })}
          </ul>
        )}
      </section>

      {onSaveSegment && selectedLabels.length > 0 && (
        <button
          type="button"
          onClick={onSaveSegment}
          className="rounded bg-emerald-600 px-3 py-1.5 text-[11px] font-medium text-white hover:bg-emerald-700"
        >
          {t("saveSegment", { count: selectedLabels.length })}
        </button>
      )}
    </aside>
  );
}
