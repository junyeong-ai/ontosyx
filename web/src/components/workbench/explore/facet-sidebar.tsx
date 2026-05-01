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
import { useMemo, useState } from "react";

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

  const [labelFilter, setLabelFilter] = useState("");

  const depths: Array<1 | 2 | 3> = [1, 2, 3];

  const visibleLabels = useMemo(() => {
    if (!overview) return [] as GraphOverview["labels"];
    const needle = labelFilter.trim().toLowerCase();
    if (!needle) return overview.labels;
    return overview.labels.filter(
      (l) =>
        l.label.toLowerCase().includes(needle) || selectedSet.has(l.label),
    );
  }, [overview, labelFilter, selectedSet]);

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

        {!loading && overview && overview.labels.length > 0 && (
          <input
            type="search"
            value={labelFilter}
            onChange={(e) => setLabelFilter(e.target.value)}
            placeholder={t("types.searchPlaceholder")}
            aria-label={t("types.searchAria")}
            className="mb-1 w-full rounded border border-zinc-200 bg-white px-1.5 py-1 text-[11px] outline-none focus:border-violet-400 dark:border-zinc-700 dark:bg-zinc-900"
          />
        )}

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
        {!loading && overview && labelFilter.trim() && visibleLabels.length === 0 && (
          <p className="py-2 text-[11px] text-muted-foreground">
            {t("types.noMatches", { query: labelFilter.trim() })}
          </p>
        )}
        {!loading && overview && (
          <ul className="flex flex-1 flex-col gap-0.5 overflow-auto pr-1 text-[11px]">
            {visibleLabels.map((l) => {
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

      {!loading && overview && overview.relationships.length > 0 && (
        <section className="border-t border-zinc-200 pt-3 dark:border-zinc-800">
          <h2 className="mb-1 text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
            {t("relationships.label")}
          </h2>
          <ul className="flex max-h-40 flex-col gap-0.5 overflow-auto pr-1 text-[11px]">
            {overview.relationships.map((r, idx) => (
              <li
                key={`${r.from_label}-${r.rel_type}-${r.to_label}-${idx}`}
                className="rounded px-2 py-1 hover:bg-zinc-100 dark:hover:bg-zinc-800"
              >
                <div className="flex items-center justify-between gap-2">
                  <span className="min-w-0 flex-1 truncate font-mono text-[10px]">
                    {r.from_label}
                    <span className="text-muted-foreground"> ─[</span>
                    <span className="font-medium">{r.rel_type}</span>
                    <span className="text-muted-foreground">]→ </span>
                    {r.to_label}
                  </span>
                  <span className="shrink-0 font-mono text-[10px] tabular-nums text-muted-foreground">
                    {r.count.toLocaleString()}
                  </span>
                </div>
              </li>
            ))}
          </ul>
          <p className="mt-1 text-[10px] italic text-muted-foreground">
            {t("relationships.readOnlyHint")}
          </p>
        </section>
      )}

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
