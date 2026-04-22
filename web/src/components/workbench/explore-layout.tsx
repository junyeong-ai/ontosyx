"use client";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { HugeiconsIcon } from "@hugeicons/react";
import {
  Search01Icon,
  ArrowRight01Icon,
  Search02Icon,
  DatabaseIcon,
} from "@hugeicons/core-free-icons";
import { z } from "zod";
import { searchGraph, fetchGraphOverview } from "@/lib/api";
import type { ExpandNeighbor, GraphOverview } from "@/lib/api/queries";
import { expandMultiHop } from "@/lib/explore/multi-hop";
import { Spinner } from "@/components/ui/spinner";
import { cn } from "@/lib/cn";
import { ErrorBoundary } from "@/components/ui/error-boundary";
import { useQueryState } from "@/hooks/use-query-state";
import { useImeAwareInput } from "@/lib/use-ime-aware-input";
import { sortKorean } from "@/lib/locale/sort";
import {
  ExploreCanvas,
  type FocusedNode,
} from "./explore/explore-canvas";
import { ExploreFacetSidebar } from "./explore/facet-sidebar";
import { toast } from "sonner";
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
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [searchInput.committedValue]);

  const [results, setResults] = useState<SearchResultNode[]>([]);
  const [loading, setLoading] = useState(false);
  const [searched, setSearched] = useState(false);

  // Graph exploration state
  const [focusedNode, setFocusedNode] = useState<FocusedNode | null>(null);
  const [neighbors, setNeighbors] = useState<ExpandNeighbor[]>([]);
  const [expanding, setExpanding] = useState(false);
  const [breadcrumb, setBreadcrumb] = useState<BreadcrumbEntry[]>([]);

  // Phase 4.4 — facet state
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
    toast.info(
      `Segment intent captured: ${selectedLabels.length} type(s). ` +
        `Persistence lands with the /segments endpoint.`,
    );
  }, [selectedLabels.length]);

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
    if (initialQueryRef.current && initialQueryRef.current.trim()) {
      runSearch();
    }
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

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
        const label = labels[0] || "Node";
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
    [expandDepth],
  );

  // ---- Select a search result ----

  const handleSelectResult = useCallback(
    (result: SearchResultNode) => {
      const label = result.labels[0] || "Node";
      const name = resolveDisplayName(result.props, label);
      setBreadcrumb([{ elementId: result.elementId, label, name }]);
      expandAndNavigate(result.elementId, result.labels, result.props, false);
    },
    [expandAndNavigate],
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
          const name = resolveDisplayName(first.props, first.labels[0] || "Node");
          setBreadcrumb([{ elementId: first.elementId, label: first.labels[0] || "Node", name }]);
          expandAndNavigate(first.elementId, first.labels, first.props, false);
        }
      } catch {
        setResults([]);
      } finally {
        setLoading(false);
      }
    },
    [expandAndNavigate, searchInput, setQuery],
  );

  // ---- Graph node click (handles both schema mode and exploration mode) ----

  const handleGraphNodeClick = useCallback(
    (nodeId: string) => {
      // Schema mode: node IDs are "schema:LabelName"
      if (nodeId.startsWith("schema:")) {
        const label = nodeId.slice("schema:".length);
        handleBrowseLabel(label);
        return;
      }

      // Exploration mode: find neighbor data and navigate
      const neighbor = neighbors.find((n) => n.element_id === nodeId);
      if (!neighbor) return;
      expandAndNavigate(
        neighbor.element_id,
        neighbor.labels,
        neighbor.props,
        true,
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
      {/* Phase 4.4 — Facet sidebar */}
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
      <div className="flex h-full w-72 shrink-0 flex-col border-r border-zinc-200 dark:border-zinc-800">
        {/* Search input */}
        <div className="border-b border-zinc-200 p-3 dark:border-zinc-800">
          <div className="flex items-center gap-1.5 rounded-md border border-zinc-200 bg-zinc-50 px-2 py-1.5 dark:border-zinc-700 dark:bg-zinc-900">
            <HugeiconsIcon
              icon={Search01Icon}
              className="h-3 w-3 text-muted-foreground"
              size="100%"
            />
            <input
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
              placeholder="Search nodes..."
              className="w-full bg-transparent text-xs text-zinc-700 outline-none placeholder:text-zinc-500 dark:text-zinc-300"
            />
            {loading && <Spinner size="xs" className="text-muted-foreground" />}
          </div>
        </div>

        {/* Results / Overview */}
        <div className="flex-1 overflow-auto">
          {/* Schema overview — shown before any search */}
          {!searched && !overviewLoading && overview && overview.labels.length > 0 && (
            <div className="p-3 space-y-4">
              {/* Stats summary */}
              <div className="flex gap-3">
                <div className="rounded bg-zinc-100 px-2 py-1 dark:bg-zinc-800">
                  <div className="text-xs font-semibold text-zinc-700 dark:text-zinc-300">
                    {overview.total_nodes.toLocaleString()}
                  </div>
                  <div className="text-[9px] text-muted-foreground">nodes</div>
                </div>
                <div className="rounded bg-zinc-100 px-2 py-1 dark:bg-zinc-800">
                  <div className="text-xs font-semibold text-zinc-700 dark:text-zinc-300">
                    {overview.total_relationships.toLocaleString()}
                  </div>
                  <div className="text-[9px] text-muted-foreground">relationships</div>
                </div>
              </div>

              {/* Node labels */}
              <div>
                <div className="mb-1.5 flex items-center gap-1.5">
                  <HugeiconsIcon icon={DatabaseIcon} className="h-3 w-3 text-muted-foreground" size="100%" />
                  <span className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
                    Node Labels
                  </span>
                </div>
                <div className="space-y-0.5">
                  {overview.labels.map(({ label, count }) => (
                    <button
                      key={label}
                      onClick={() => handleBrowseLabel(label)}
                      className="flex w-full items-center justify-between rounded px-2 py-1.5 text-left transition-colors hover:bg-zinc-100 dark:hover:bg-zinc-800"
                    >
                      <div className="flex items-center gap-2">
                        <span
                          className="h-2.5 w-2.5 rounded-full shrink-0"
                          style={{ backgroundColor: resolveNodeColor(label, false) }}
                        />
                        <span className="text-xs font-medium text-zinc-700 dark:text-zinc-300">
                          {label}
                        </span>
                      </div>
                      <span className="text-[10px] tabular-nums text-muted-foreground">
                        {count.toLocaleString()}
                      </span>
                    </button>
                  ))}
                </div>
              </div>

              {/* Relationship patterns */}
              {overview.relationships.length > 0 && (
                <div>
                  <div className="mb-1.5 text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
                    Relationship Patterns
                  </div>
                  <div className="space-y-0.5">
                    {overview.relationships.slice(0, 15).map((rel) => (
                      <div
                        key={`${rel.from_label}-${rel.rel_type}-${rel.to_label}`}
                        className="flex items-center gap-1 rounded px-2 py-1 text-[10px]"
                      >
                        <span className="font-medium text-zinc-600 dark:text-muted-foreground">{rel.from_label}</span>
                        <span className="text-muted-foreground">→</span>
                        <span className="font-mono text-muted-foreground">{rel.rel_type}</span>
                        <span className="text-muted-foreground">→</span>
                        <span className="font-medium text-zinc-600 dark:text-muted-foreground">{rel.to_label}</span>
                        <span className="ml-auto tabular-nums text-muted-foreground">{rel.count.toLocaleString()}</span>
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
              <Spinner size="sm" className="text-muted-foreground" />
              <p className="text-xs text-muted-foreground">Loading graph schema...</p>
            </div>
          )}

          {/* Empty graph fallback */}
          {!searched && !overviewLoading && (!overview || overview.labels.length === 0) && (
            <div className="flex flex-col items-center gap-3 px-4 py-8 text-center">
              <div className="flex h-10 w-10 items-center justify-center rounded-full bg-emerald-50 dark:bg-emerald-950/30">
                <HugeiconsIcon icon={Search02Icon} className="h-4 w-4 text-emerald-500" size="100%" />
              </div>
              <div>
                <p className="text-sm font-medium text-zinc-700 dark:text-zinc-300">No graph data yet</p>
                <p className="mt-1 text-xs text-muted-foreground">
                  Deploy a schema and load data from Design mode to start exploring.
                </p>
              </div>
              <button
                onClick={() => router.push("/design")}
                className="rounded-lg border border-emerald-200 bg-emerald-50 px-4 py-2 text-xs font-medium text-emerald-700 transition-colors hover:bg-emerald-100 dark:border-emerald-800 dark:bg-emerald-950/30 dark:text-emerald-400 dark:hover:bg-emerald-950/50"
              >
                Switch to Design
              </button>
            </div>
          )}

          {/* Search results */}
          {searched && !loading && results.length === 0 && (
            <div className="px-4 py-8 text-center text-xs text-muted-foreground">
              No results found
            </div>
          )}
          {results.map((result, i) => (
            <button
              key={result.elementId || i}
              onClick={() => handleSelectResult(result)}
              className={cn(
                "flex w-full items-start gap-2 border-b border-zinc-100 px-3 py-2 text-left transition-colors dark:border-zinc-800",
                focusedNode?.elementId === result.elementId
                  ? "bg-emerald-50 dark:bg-emerald-950/30"
                  : "hover:bg-zinc-50 dark:hover:bg-zinc-800",
              )}
            >
              <div className="flex flex-wrap gap-1">
                {result.labels.map((l) => (
                  <span
                    key={l}
                    className="rounded bg-zinc-100 px-1 py-0.5 text-[9px] font-medium text-zinc-600 dark:bg-zinc-800 dark:text-muted-foreground"
                  >
                    {l}
                  </span>
                ))}
              </div>
              <span className="min-w-0 flex-1 truncate text-xs font-medium text-zinc-700 dark:text-zinc-300">
                {resolveDisplayName(result.props)}
              </span>
            </button>
          ))}

          {/* Back to overview — reset all exploration state */}
          {searched && (
            <button
              onClick={() => {
                setSearched(false);
                setResults([]);
                searchInput.setValue("");
                setQuery("");
                setFocusedNode(null);
                setNeighbors([]);
                setBreadcrumb([]);
              }}
              className="w-full px-3 py-2 text-left text-[10px] text-muted-foreground transition-colors hover:text-zinc-600 dark:hover:text-zinc-300"
            >
              ← Back to overview
            </button>
          )}
        </div>
      </div>

      {/* Center: Graph Visualization */}
      <div className="flex flex-1 flex-col overflow-hidden bg-zinc-50 dark:bg-zinc-950">
        {/* Breadcrumb navigation */}
        {breadcrumb.length > 0 && (
          <div className="flex items-center gap-1 border-b border-zinc-200 px-3 py-1.5 dark:border-zinc-800">
            {breadcrumb.map((entry, i) => (
              <span key={`${entry.elementId}-${i}`} className="flex items-center gap-1">
                {i > 0 && (
                  <HugeiconsIcon
                    icon={ArrowRight01Icon}
                    className="h-2.5 w-2.5 text-muted-foreground"
                    size="100%"
                  />
                )}
                <button
                  onClick={() => handleBreadcrumbClick(i)}
                  className={cn(
                    "flex items-center gap-1 rounded px-1.5 py-0.5 text-[10px] transition-colors",
                    i === breadcrumb.length - 1
                      ? "bg-emerald-100 font-medium text-emerald-700 dark:bg-emerald-900/50 dark:text-emerald-400"
                      : "text-muted-foreground hover:bg-zinc-100 hover:text-zinc-700 dark:hover:bg-zinc-800 dark:hover:text-zinc-300",
                  )}
                >
                  <span className="text-[9px] text-muted-foreground">
                    {entry.label}:
                  </span>
                  <span className="max-w-24 truncate">{entry.name}</span>
                </button>
              </span>
            ))}
            {expanding && <Spinner size="xs" className="ml-1 text-muted-foreground" />}
          </div>
        )}

        {/* Graph view */}
        <div className="relative flex-1">
          {expanding && !focusedNode && (
            <div className="absolute inset-0 z-10 flex items-center justify-center bg-zinc-50/80 dark:bg-zinc-950/80">
              <Spinner size="md" className="text-emerald-500" />
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
            <div className="absolute bottom-2 right-2 rounded bg-zinc-900/70 px-2 py-1 text-[10px] text-zinc-300">
              {neighbors.length} neighbor{neighbors.length !== 1 ? "s" : ""}
            </div>
          )}
        </div>
      </div>

      {/* Right: Detail panel */}
      <div className="flex h-full w-80 shrink-0 flex-col border-l border-zinc-200 dark:border-zinc-800">
        {/* Properties section */}
        <div className="flex h-7 items-center border-b border-zinc-200 px-3 dark:border-zinc-800">
          <span className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
            Properties
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
                    className="rounded bg-emerald-100 px-1.5 py-0.5 text-[10px] font-medium text-emerald-700 dark:bg-emerald-900 dark:text-emerald-400"
                  >
                    {l}
                  </span>
                ))}
              </div>
              {/* Properties */}
              <div className="space-y-1 pt-2">
                {Object.entries(focusedNode.props).map(([key, value]) => (
                  <div key={key} className="flex items-start gap-2 text-xs">
                    <span className="shrink-0 font-medium text-zinc-500 dark:text-muted-foreground">
                      {key}
                    </span>
                    <span className="min-w-0 break-all text-zinc-700 dark:text-zinc-300">
                      {formatPropertyValue(value)}
                    </span>
                  </div>
                ))}
              </div>
              {/* Element ID */}
              <div className="border-t border-zinc-100 pt-2 dark:border-zinc-800">
                <span className="text-[9px] text-muted-foreground">
                  ID: {focusedNode.elementId}
                </span>
              </div>

              {/* Relationships section */}
              {groupedRelationships.length > 0 && (
                <div className="border-t border-zinc-100 pt-3 dark:border-zinc-800">
                  <span className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
                    Relationships
                  </span>
                  <div className="mt-2 space-y-1.5">
                    {groupedRelationships.map((group) => (
                      <div key={`${group.direction}:${group.type}`}>
                        {/* Group header */}
                        <div className="flex items-center gap-1.5 text-[10px]">
                          <span
                            className={cn(
                              "text-[9px] font-bold",
                              group.direction === "outgoing"
                                ? "text-blue-400"
                                : "text-amber-400",
                            )}
                          >
                            {group.direction === "outgoing" ? "\u2192" : "\u2190"}
                          </span>
                          <span className="font-mono font-medium text-zinc-600 dark:text-muted-foreground">
                            {group.type}
                          </span>
                          <span className="text-muted-foreground">
                            ({group.items.length})
                          </span>
                        </div>
                        {/* Individual neighbors */}
                        <div className="ml-4 mt-0.5 space-y-0.5">
                          {group.items.map((neighbor) => (
                            <button
                              key={neighbor.element_id}
                              onClick={() =>
                                handleRelationshipClick(neighbor)
                              }
                              className="flex w-full items-center gap-1.5 rounded px-1.5 py-0.5 text-left text-[10px] transition-colors hover:bg-zinc-100 dark:hover:bg-zinc-800"
                            >
                              <span className="rounded bg-zinc-100 px-1 py-0.5 text-[8px] font-medium text-zinc-500 dark:bg-zinc-800 dark:text-muted-foreground">
                                {neighbor.labels[0] || "Node"}
                              </span>
                              <span className="min-w-0 flex-1 truncate text-zinc-600 dark:text-zinc-300">
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
            <div className="flex h-full items-center justify-center text-xs text-muted-foreground">
              Select an entity to view properties
            </div>
          )}
        </div>
      </div>
    </div>
    </ErrorBoundary>
  );
}
