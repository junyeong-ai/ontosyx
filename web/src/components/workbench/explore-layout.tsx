"use client";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { useTranslations } from "next-intl";
import { RouteHeading } from "@/components/layout/route-heading";
import { ArrowRight, Database, Search } from "lucide-react";
import { SearchCheck } from "lucide-react";
import { z } from "zod";
import { searchGraph, fetchGraphOverview } from "@/lib/api";
import type { ExpandNeighbor, GraphOverview } from "@/lib/api/queries";
import { expandMultiHop } from "@/lib/explore/multi-hop";
import { Spinner } from "@/components/ui/spinner";
import { SearchInput } from "@/components/ui/form-input";
import { cn } from "@/lib/cn";
import { ErrorBoundary } from "@/components/ui/error-boundary";
import { useQueryState } from "@/hooks/use-query-state";
import { useImeAwareInput } from "@/hooks/use-ime-aware-input";
import { sortKorean } from "@/lib/locale/sort";
import {
  ExploreCanvas,
  type FocusedNode,
  type NodeClickModifiers,
} from "./explore/explore-canvas";
import { ExploreFacetSidebar } from "./explore/facet-sidebar";
import { toast } from "@/components/ui/toast";
import { useFormatters } from "@/hooks/use-formatters";
import {
  type SearchResultNode,
  toSearchResultNodes,
  resolveDisplayName,
  resolveNodeColor,
  formatPropertyValue,
} from "./explore/graph-utils";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface BreadcrumbEntry {
  elementId: string;
  label: string;
  name: string;
}

// ---------------------------------------------------------------------------
// ExploreLayout — graph data exploration mode
// ---------------------------------------------------------------------------

export function ExploreLayout() {
  const t = useTranslations("workbench.explore");
  const fmt = useFormatters();
  const router = useRouter();

  // Schema overview state
  const [overview, setOverview] = useState<GraphOverview | null>(null);
  const [overviewLoading, setOverviewLoading] = useState(false);

  // Search state — persisted to ?q= so reloads / shares survive.
  // Debounced by the hook; `q` updates immediately for the input,
  // URL writes coalesce after 200ms of idle.
  const [query, setQuery] = useQueryState("q", {
    default: "",
    parser: z.string(),
  });
  // IME-aware input: Hangul composition should not trigger mid-jamo state
  // updates (e.g. "한" composing as "ㅎ" → "하" → "한"). The committed value
  // is what we persist to the URL.
  const searchInput = useImeAwareInput(query);
  // Propagate committed (non-composing) values to the URL-state setter.
  useEffect(() => {
    if (searchInput.committedValue !== query) {
      setQuery(searchInput.committedValue);
    }
  }, [searchInput.committedValue, query, setQuery]);

  const [results, setResults] = useState<SearchResultNode[]>([]);
  const [loading, setLoading] = useState(false);
  const [searched, setSearched] = useState(false);

  // Graph exploration state
  const [focusedNode, setFocusedNode] = useState<FocusedNode | null>(null);
  const [neighbors, setNeighbors] = useState<ExpandNeighbor[]>([]);
  const [expanding, setExpanding] = useState(false);
  const [breadcrumb, setBreadcrumb] = useState<BreadcrumbEntry[]>([]);

  const [expandDepth, setExpandDepth] = useState<1 | 2 | 3>(1);
  const [selectedLabels, setSelectedLabels] = useState<string[]>([]);
  const toggleLabel = useCallback((label: string) => {
    setSelectedLabels((prev) =>
      prev.includes(label) ? prev.filter((l) => l !== label) : [...prev, label],
    );
  }, []);
  const clearLabels = useCallback(() => setSelectedLabels([]), []);
  const handleSaveSegment = useCallback(() => {
    // Segment creation is a two-step: construct SegmentDef from the
    // current selection + post through the (forthcoming) /segments
    // endpoint. Until that endpoint lands, we surface the intent so
    // the operator sees the button wires up and the selection is
    // preserved for the follow-up session.
    toast.info(t("segmentToast", { count: selectedLabels.length }));
  }, [selectedLabels.length, t]);

  // ---- Fetch schema overview on mount ----

  useEffect(() => {
    let cancelled = false;
    setOverviewLoading(true);

    fetchGraphOverview()
      .then((data) => {
        if (!cancelled) setOverview(data);
      })
      .catch(() => {
        if (!cancelled) setOverview(null);
      })
      .finally(() => {
        if (!cancelled) setOverviewLoading(false);
      });

    return () => { cancelled = true; };
  }, []);

  // ---- Search ----

  const runSearch = useCallback(async () => {
    const q = query.trim();
    if (!q) return;
    setLoading(true);
    setSearched(true);
    try {
      const result = await searchGraph(q, 50);
      setResults(toSearchResultNodes(result));
      setFocusedNode(null);
      setNeighbors([]);
      setBreadcrumb([]);
    } catch {
      setResults([]);
    } finally {
      setLoading(false);
    }
  }, [query]);

  // Rehydrate from URL: if someone opens a deep link with `?q=...`, kick
  // off the search once on mount. Only runs once per mount even if `query`
  // changes later — subsequent keystroke-driven searches go through
  // `runSearch` via Enter.
  const initialQueryRef = useRef(query);
  useEffect(() => {
    if (initialQueryRef.current?.trim()) {
      runSearch();
    }
  }, [runSearch]);

  // ---- Expand a node ----

  const expandAndNavigate = useCallback(
    async (
      elementId: string,
      labels: string[],
      props: Record<string, unknown>,
      appendBreadcrumb: boolean,
      overrideDepth?: 1 | 2 | 3,
    ) => {
      setExpanding(true);
      const node: FocusedNode = { elementId, labels, props };
      setFocusedNode(node);

      if (appendBreadcrumb) {
        const label = labels[0] || t("nodeFallback");
        const name = resolveDisplayName(props, label);
        setBreadcrumb((prev) => [...prev, { elementId, label, name }]);
      }

      try {
        const neighbors = await expandMultiHop(elementId, {
          depth: overrideDepth ?? expandDepth,
          maxNodes: 100,
          perHopLimit: 50,
        });
        setNeighbors(neighbors);
      } catch {
        setNeighbors([]);
      } finally {
        setExpanding(false);
      }
    },
    [expandDepth, t],
  );

  // ---- Select a search result ----

  const handleSelectResult = useCallback(
    (result: SearchResultNode) => {
      const label = result.labels[0] || t("nodeFallback");
      const name = resolveDisplayName(result.props, label);
      setBreadcrumb([{ elementId: result.elementId, label, name }]);
      expandAndNavigate(result.elementId, result.labels, result.props, false);
    },
    [expandAndNavigate, t],
  );

  // ---- Browse by label (overview / schema graph entry point) ----

  const handleBrowseLabel = useCallback(
    async (label: string) => {
      setLoading(true);
      setSearched(true);
      searchInput.setValue("");
      setQuery("");
      try {
        // Wildcard "*" + label filter = match all nodes of this label type
        const result = await searchGraph("*", 50, [label]);
        const hits = toSearchResultNodes(result);

        setResults(hits);
        setFocusedNode(null);
        setNeighbors([]);
        setBreadcrumb([]);
        // Auto-select first result for immediate graph view
        if (hits.length > 0) {
          const first = hits[0];
          const name = resolveDisplayName(first.props, first.labels[0] || t("nodeFallback"));
          setBreadcrumb([{ elementId: first.elementId, label: first.labels[0] || t("nodeFallback"), name }]);
          expandAndNavigate(first.elementId, first.labels, first.props, false);
        }
      } catch {
        setResults([]);
      } finally {
        setLoading(false);
      }
    },
    [expandAndNavigate, searchInput, setQuery, t],
  );

  // ---- Graph node click (handles both schema mode and exploration mode) ----

  const handleGraphNodeClick = useCallback(
    (nodeId: string, modifiers?: NodeClickModifiers) => {
      // Schema mode: node IDs are "schema:LabelName". Modifiers are
      // ignored in schema mode — the browse path doesn't expand by
      // depth, it drops the user on a label listing.
      if (nodeId.startsWith("schema:")) {
        const label = nodeId.slice("schema:".length);
        handleBrowseLabel(label);
        return;
      }

      // Exploration mode: find neighbor data and navigate. When
      // Cmd/Ctrl was held at click time the modifier carries a
      // one-shot `forceDepth` that overrides the sidebar's radio
      // setting for this single expansion.
      const neighbor = neighbors.find((n) => n.element_id === nodeId);
      if (!neighbor) return;
      expandAndNavigate(
        neighbor.element_id,
        neighbor.labels,
        neighbor.props,
        true,
        modifiers?.forceDepth,
      );
    },
    [neighbors, expandAndNavigate, handleBrowseLabel],
  );

  // ---- Breadcrumb click ----

  const handleBreadcrumbClick = useCallback(
    (index: number) => {
      const entry = breadcrumb[index];
      if (!entry) return;
      setBreadcrumb(breadcrumb.slice(0, index + 1));
      expandAndNavigate(entry.elementId, [entry.label], {}, false);
    },
    [breadcrumb, expandAndNavigate],
  );

  // ---- Relationship click in detail panel ----

  const handleRelationshipClick = useCallback(
    (neighbor: ExpandNeighbor) => {
      expandAndNavigate(
        neighbor.element_id,
        neighbor.labels,
        neighbor.props,
        true,
      );
    },
    [expandAndNavigate],
  );

  // ---- Grouped relationships for detail panel ----

  const groupedRelationships = useMemo(() => {
    if (!focusedNode || neighbors.length === 0) return [];

    const groups = new Map<
      string,
      { type: string; direction: "incoming" | "outgoing"; items: ExpandNeighbor[] }
    >();

    for (const n of neighbors) {
      const key = `${n.direction}:${n.relationship_type}`;
      let group = groups.get(key);
      if (!group) {
        group = {
          type: n.relationship_type,
          direction: n.direction as "incoming" | "outgoing",
          items: [],
        };
        groups.set(key, group);
      }
      group.items.push(n);
    }

    // Korean-aware ordering — relationship types may contain Hangul labels
    // so we use the shared collator instead of raw localeCompare.
    return sortKorean(Array.from(groups.values()), (g) => g.type);
  }, [focusedNode, neighbors]);

  return (
    <ErrorBoundary name="Explore">
    <div className="flex h-full">
      <RouteHeading route="explore" />
      <ExploreFacetSidebar
        overview={overview}
        loading={overviewLoading}
        selectedLabels={selectedLabels}
        onToggleLabel={toggleLabel}
        onClearLabels={clearLabels}
        expandDepth={expandDepth}
        onChangeDepth={setExpandDepth}
        onSaveSegment={handleSaveSegment}
      />

      {/* Left: Search + Results */}
      <div className="flex h-full w-72 shrink-0 flex-col border-e border-divider">
        {/* Search input */}
        <div className="border-b border-divider p-3">
          <SearchInput
            type="search"
            value={searchInput.value}
            onChange={searchInput.bind.onChange}
            onCompositionStart={searchInput.bind.onCompositionStart}
            onCompositionEnd={searchInput.bind.onCompositionEnd}
            onKeyDown={(e) => {
              // Enter should only fire when NOT mid-composition. Browsers
              // fire `Enter` at 229 keyCode during IME commit — checking
              // `isComposing` avoids an errant search.
              if (e.key === "Enter" && !e.nativeEvent.isComposing) runSearch();
            }}
            placeholder={t("search.placeholder")}
            aria-label={t("search.placeholder")}
            density="settings"
            leadingIcon={Search}
            trailing={
              loading ? (
                <Spinner size="xs" className="text-foreground-muted" />
              ) : null
            }
          />
        </div>

        {/* Results / Overview */}
        <div className="flex-1 overflow-auto">
          {/* Schema overview — shown before any search */}
          {!searched && !overviewLoading && overview && overview.labels.length > 0 && (
            <div className="p-3 space-y-4">
              {/* Stats summary */}
              <div className="flex gap-3">
                <div className="rounded bg-surface-inset px-2 py-1">
                  <div className="text-xs font-semibold text-foreground">
                    {fmt.number(overview.total_nodes)}
                  </div>
                  <div className="text-2xs text-foreground-muted">{t("stats.nodes")}</div>
                </div>
                <div className="rounded bg-surface-inset px-2 py-1">
                  <div className="text-xs font-semibold text-foreground">
                    {fmt.number(overview.total_relationships)}
                  </div>
                  <div className="text-2xs text-foreground-muted">{t("stats.relationships")}</div>
                </div>
              </div>

              {/* Node labels */}
              <div>
                <div className="mb-1.5 flex items-center gap-1.5">
                  <Database className="h-3 w-3 text-foreground-muted" />
                  <span className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
                    {t("overview.nodeLabels")}
                  </span>
                </div>
                <div className="space-y-0.5">
                  {overview.labels.map(({ label, count }) => (
                    <button type="button"
                      key={label}
                      onClick={() => handleBrowseLabel(label)}
                      className="flex w-full items-center justify-between rounded px-2 py-1.5 text-start transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] hover:bg-surface-inset"
                    >
                      <div className="flex items-center gap-2">
                        <span
                          className="h-2.5 w-2.5 rounded-full shrink-0"
                          style={{ backgroundColor: resolveNodeColor(label, false) }}
                        />
                        <span className="text-xs font-medium text-foreground">
                          {label}
                        </span>
                      </div>
                      <span className="text-2xs tabular-nums text-foreground-muted">
                        {fmt.number(count)}
                      </span>
                    </button>
                  ))}
                </div>
              </div>

              {/* Relationship patterns */}
              {overview.relationships.length > 0 && (
                <div>
                  <div className="mb-1.5 text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
                    {t("overview.relationshipPatterns")}
                  </div>
                  <div className="space-y-0.5">
                    {overview.relationships.slice(0, 15).map((rel) => (
                      <div
                        key={`${rel.from_label}-${rel.rel_type}-${rel.to_label}`}
                        className="flex items-center gap-1 rounded px-2 py-1 text-2xs"
                      >
                        <span className="font-medium text-foreground">{rel.from_label}</span>
                        <span className="text-foreground-muted">→</span>
                        <span className="font-mono text-foreground-muted">{rel.rel_type}</span>
                        <span className="text-foreground-muted">→</span>
                        <span className="font-medium text-foreground">{rel.to_label}</span>
                        <span className="ms-auto tabular-nums text-foreground-muted">{fmt.number(rel.count)}</span>
                      </div>
                    ))}
                  </div>
                </div>
              )}
            </div>
          )}

          {/* Overview loading */}
          {!searched && overviewLoading && (
            <div className="flex flex-col items-center gap-2 px-4 py-8 text-center">
              <Spinner size="sm" className="text-foreground-muted" />
              <p className="text-xs text-foreground-muted">{t("overview.loading")}</p>
            </div>
          )}

          {/* Empty graph fallback */}
          {!searched && !overviewLoading && (!overview || overview.labels.length === 0) && (
            <div className="flex flex-col items-center gap-3 px-4 py-8 text-center">
              <div className="flex h-10 w-10 items-center justify-center rounded-full bg-brand-surface">
                <SearchCheck className="h-4 w-4 text-brand-foreground" />
              </div>
              <div>
                <p className="text-sm font-medium text-foreground">{t("empty.title")}</p>
                <p className="mt-1 text-xs text-foreground-muted">
                  {t("empty.description")}
                </p>
              </div>
              <button type="button"
                onClick={() => router.push("/design")}
                className="rounded-lg border border-brand-border bg-brand-surface px-4 py-2 text-xs font-medium text-brand-foreground transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] hover:bg-brand-surface-strong/30"
              >
                {t("empty.switchToDesign")}
              </button>
            </div>
          )}

          {/* Search results */}
          {searched && !loading && results.length === 0 && (
            <div className="px-4 py-8 text-center text-xs text-foreground-muted">
              {t("search.noResults")}
            </div>
          )}
          {results.map((result, i) => (
            <button type="button"
              key={result.elementId || i}
              onClick={() => handleSelectResult(result)}
              className={cn(
                "flex w-full items-start gap-2 border-b border-divider-soft px-3 py-2 text-start transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)]",
                focusedNode?.elementId === result.elementId
                  ? "bg-brand-surface"
                  : "hover:bg-surface-raised",
              )}
            >
              <div className="flex flex-wrap gap-1">
                {result.labels.map((l) => (
                  <span
                    key={l}
                    className="rounded bg-surface-inset px-1 py-0.5 text-2xs font-medium text-foreground"
                  >
                    {l}
                  </span>
                ))}
              </div>
              <span className="min-w-0 flex-1 truncate text-xs font-medium text-foreground">
                {resolveDisplayName(result.props)}
              </span>
            </button>
          ))}

          {/* Back to overview — reset all exploration state */}
          {searched && (
            <button type="button"
              onClick={() => {
                setSearched(false);
                setResults([]);
                searchInput.setValue("");
                setQuery("");
                setFocusedNode(null);
                setNeighbors([]);
                setBreadcrumb([]);
              }}
              className="w-full px-3 py-2 text-start text-2xs text-foreground-muted transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] hover:text-foreground-muted"
            >
              {t("backToOverview")}
            </button>
          )}
        </div>
      </div>

      {/* Center: Graph Visualization */}
      <div className="flex flex-1 flex-col overflow-hidden bg-surface-raised">
        {/* Breadcrumb navigation */}
        {breadcrumb.length > 0 && (
          <div className="flex items-center gap-1 border-b border-divider px-3 py-1.5">
            {breadcrumb.map((entry, i) => (
              <span key={`${entry.elementId}-${i}`} className="flex items-center gap-1">
                {i > 0 && (
                  <ArrowRight className="h-2.5 w-2.5 text-foreground-muted" />
                )}
                <button type="button"
                  onClick={() => handleBreadcrumbClick(i)}
                  className={cn(
                    "flex items-center gap-1 rounded px-1.5 py-0.5 text-2xs transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)]",
                    i === breadcrumb.length - 1
                      ? "bg-brand-surface-strong font-medium text-brand-foreground-strong"
                      : "text-foreground-muted hover:bg-surface-inset hover:text-foreground-muted",
                  )}
                >
                  <span className="text-2xs text-foreground-muted">
                    {entry.label}:
                  </span>
                  <span className="max-w-24 truncate">{entry.name}</span>
                </button>
              </span>
            ))}
            {expanding && <Spinner size="xs" className="ms-1 text-foreground-muted" />}
          </div>
        )}

        {/* Graph view */}
        <div className="relative flex-1">
          {expanding && !focusedNode && (
            <div className="absolute inset-0 z-canvas flex items-center justify-center bg-surface-raised/80">
              <Spinner size="md" className="text-brand-foreground" />
            </div>
          )}
          <ExploreCanvas
            focusedNode={focusedNode}
            neighbors={neighbors}
            schemaOverview={overview}
            onNodeClick={handleGraphNodeClick}
          />
          {/* Stats bar */}
          {focusedNode && neighbors.length > 0 && (
            <div className="absolute bottom-2 end-2 rounded bg-surface-base/70 px-2 py-1 text-2xs text-foreground-muted">
              {t("neighborCount", { count: neighbors.length })}
            </div>
          )}
        </div>
      </div>

      {/* Right: Detail panel */}
      <div className="flex h-full w-80 shrink-0 flex-col border-s border-divider">
        {/* Properties section */}
        <div className="flex h-7 items-center border-b border-divider px-3">
          <span className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
            {t("propertiesHeading")}
          </span>
        </div>
        <div className="flex-1 overflow-y-auto">
          {focusedNode ? (
            <div className="space-y-2 p-3">
              {/* Labels */}
              <div className="flex flex-wrap gap-1">
                {focusedNode.labels.map((l) => (
                  <span
                    key={l}
                    className="rounded bg-brand-surface-strong px-1.5 py-0.5 text-2xs font-medium text-brand-foreground-strong"
                  >
                    {l}
                  </span>
                ))}
              </div>
              {/* Properties */}
              <div className="space-y-1 pt-2">
                {Object.entries(focusedNode.props).map(([key, value]) => (
                  <div key={key} className="flex items-start gap-2 text-xs">
                    <span className="shrink-0 font-medium text-foreground-muted">
                      {key}
                    </span>
                    <span className="min-w-0 break-all text-foreground">
                      {formatPropertyValue(value)}
                    </span>
                  </div>
                ))}
              </div>
              {/* Element ID */}
              <div className="border-t border-divider-soft pt-2">
                <span className="text-2xs text-foreground-muted">
                  ID: {focusedNode.elementId}
                </span>
              </div>

              {/* Relationships section */}
              {groupedRelationships.length > 0 && (
                <div className="border-t border-divider-soft pt-3">
                  <span className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
                    {t("relationshipsHeading")}
                  </span>
                  <div className="mt-2 space-y-1.5">
                    {groupedRelationships.map((group) => (
                      <div key={`${group.direction}:${group.type}`}>
                        {/* Group header */}
                        <div className="flex items-center gap-1.5 text-2xs">
                          <span
                            className={cn(
                              "text-2xs font-bold",
                              group.direction === "outgoing"
                                ? "text-info-foreground"
                                : "text-warning-foreground",
                            )}
                          >
                            {group.direction === "outgoing" ? "\u2192" : "\u2190"}
                          </span>
                          <span className="font-mono font-medium text-foreground">
                            {group.type}
                          </span>
                          <span className="text-foreground-muted">
                            ({group.items.length})
                          </span>
                        </div>
                        {/* Individual neighbors */}
                        <div className="ms-4 mt-0.5 space-y-0.5">
                          {group.items.map((neighbor) => (
                            <button type="button"
                              key={neighbor.element_id}
                              onClick={() =>
                                handleRelationshipClick(neighbor)
                              }
                              className="flex w-full items-center gap-1.5 rounded px-1.5 py-0.5 text-start text-2xs transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] hover:bg-surface-inset"
                            >
                              <span className="rounded bg-surface-inset px-1 py-0.5 text-2xs font-medium text-foreground-muted">
                                {neighbor.labels[0] || t("nodeFallback")}
                              </span>
                              <span className="min-w-0 flex-1 truncate text-foreground-muted">
                                {resolveDisplayName(neighbor.props)}
                              </span>
                            </button>
                          ))}
                        </div>
                      </div>
                    ))}
                  </div>
                </div>
              )}
            </div>
          ) : (
            <div className="flex h-full items-center justify-center text-xs text-foreground-muted">
              {t("selectEntityHint")}
            </div>
          )}
        </div>
      </div>
    </div>
    </ErrorBoundary>
  );
}
