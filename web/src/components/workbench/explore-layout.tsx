"use client";

import { useCallback, useEffect, useMemo, useState } from "react";
import { HugeiconsIcon } from "@hugeicons/react";
import {
  Search01Icon,
  ArrowRight01Icon,
  Search02Icon,
  DatabaseIcon,
} from "@hugeicons/core-free-icons";
import { searchGraph, expandNode, fetchGraphOverview } from "@/lib/api";
import type { ExpandNeighbor, GraphOverview } from "@/lib/api/queries";
import { Spinner } from "@/components/ui/spinner";
import { cn } from "@/lib/cn";
import { ErrorBoundary } from "@/components/ui/error-boundary";
import {
  ExploreGraphView,
  type FocusedNode,
} from "./explore/graph-view";
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

  // Schema overview state
  const [overview, setOverview] = useState<GraphOverview | null>(null);
  const [overviewLoading, setOverviewLoading] = useState(false);

  // Search state
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<SearchResultNode[]>([]);
  const [loading, setLoading] = useState(false);
  const [searched, setSearched] = useState(false);

  // Graph exploration state
  const [focusedNode, setFocusedNode] = useState<FocusedNode | null>(null);
  const [neighbors, setNeighbors] = useState<ExpandNeighbor[]>([]);
  const [expanding, setExpanding] = useState(false);
  const [breadcrumb, setBreadcrumb] = useState<BreadcrumbEntry[]>([]);

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

  // ---- Expand a node ----

  const expandAndNavigate = useCallback(
    async (
      elementId: string,
      labels: string[],
      props: Record<string, unknown>,
      appendBreadcrumb: boolean,
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
        const result = await expandNode(elementId, 50);
        setNeighbors(result.neighbors);
      } catch {
        setNeighbors([]);
      } finally {
        setExpanding(false);
      }
    },
    [],
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
    [expandAndNavigate],
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

    return Array.from(groups.values()).sort((a, b) =>
      a.type.localeCompare(b.type),
    );
  }, [focusedNode, neighbors]);

  return (
    <ErrorBoundary name="Explore">
    <div className="flex h-full">
      {/* Left: Search + Results */}
      <div className="flex h-full w-72 shrink-0 flex-col border-r border-zinc-200 dark:border-zinc-800">
        {/* Search input */}
        <div className="border-b border-zinc-200 p-3 dark:border-zinc-800">
          <div className="flex items-center gap-1.5 rounded-md border border-zinc-200 bg-zinc-50 px-2 py-1.5 dark:border-zinc-700 dark:bg-zinc-900">
            <HugeiconsIcon
              icon={Search01Icon}
              className="h-3 w-3 text-zinc-400"
              size="100%"
            />
            <input
              type="text"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") runSearch();
              }}
              placeholder="Search nodes..."
              className="w-full bg-transparent text-xs text-zinc-700 outline-none placeholder:text-zinc-500 dark:text-zinc-300"
            />
            {loading && <Spinner size="xs" className="text-zinc-400" />}
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
                  <div className="text-[9px] text-zinc-400">nodes</div>
                </div>
                <div className="rounded bg-zinc-100 px-2 py-1 dark:bg-zinc-800">
                  <div className="text-xs font-semibold text-zinc-700 dark:text-zinc-300">
                    {overview.total_relationships.toLocaleString()}
                  </div>
                  <div className="text-[9px] text-zinc-400">relationships</div>
                </div>
              </div>

              {/* Node labels */}
              <div>
                <div className="mb-1.5 flex items-center gap-1.5">
                  <HugeiconsIcon icon={DatabaseIcon} className="h-3 w-3 text-zinc-400" size="100%" />
                  <span className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
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
                      <span className="text-[10px] tabular-nums text-zinc-400">
                        {count.toLocaleString()}
                      </span>
                    </button>
                  ))}
                </div>
              </div>

              {/* Relationship patterns */}
              {overview.relationships.length > 0 && (
                <div>
                  <div className="mb-1.5 text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
                    Relationship Patterns
                  </div>
                  <div className="space-y-0.5">
                    {overview.relationships.slice(0, 15).map((rel) => (
                      <div
                        key={`${rel.from_label}-${rel.rel_type}-${rel.to_label}`}
                        className="flex items-center gap-1 rounded px-2 py-1 text-[10px]"
                      >
                        <span className="font-medium text-zinc-600 dark:text-zinc-400">{rel.from_label}</span>
                        <span className="text-zinc-400">→</span>
                        <span className="font-mono text-zinc-500">{rel.rel_type}</span>
                        <span className="text-zinc-400">→</span>
                        <span className="font-medium text-zinc-600 dark:text-zinc-400">{rel.to_label}</span>
                        <span className="ml-auto tabular-nums text-zinc-400">{rel.count.toLocaleString()}</span>
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
              <Spinner size="sm" className="text-zinc-400" />
              <p className="text-xs text-zinc-400">Loading graph schema...</p>
            </div>
          )}

          {/* Empty graph fallback */}
          {!searched && !overviewLoading && (!overview || overview.labels.length === 0) && (
            <div className="flex flex-col items-center gap-2 px-4 py-8 text-center">
              <HugeiconsIcon icon={Search02Icon} className="h-5 w-5 text-zinc-300 dark:text-zinc-600" size="100%" />
              <p className="text-xs text-zinc-400">Search for entities in your graph</p>
            </div>
          )}

          {/* Search results */}
          {searched && !loading && results.length === 0 && (
            <div className="px-4 py-8 text-center text-xs text-zinc-400">
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
                    className="rounded bg-zinc-100 px-1 py-0.5 text-[9px] font-medium text-zinc-600 dark:bg-zinc-800 dark:text-zinc-400"
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
                setQuery("");
                setFocusedNode(null);
                setNeighbors([]);
                setBreadcrumb([]);
              }}
              className="w-full px-3 py-2 text-left text-[10px] text-zinc-400 transition-colors hover:text-zinc-600 dark:hover:text-zinc-300"
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
                    className="h-2.5 w-2.5 text-zinc-400"
                    size="100%"
                  />
                )}
                <button
                  onClick={() => handleBreadcrumbClick(i)}
                  className={cn(
                    "flex items-center gap-1 rounded px-1.5 py-0.5 text-[10px] transition-colors",
                    i === breadcrumb.length - 1
                      ? "bg-emerald-100 font-medium text-emerald-700 dark:bg-emerald-900/50 dark:text-emerald-400"
                      : "text-zinc-500 hover:bg-zinc-100 hover:text-zinc-700 dark:hover:bg-zinc-800 dark:hover:text-zinc-300",
                  )}
                >
                  <span className="text-[9px] text-zinc-400">
                    {entry.label}:
                  </span>
                  <span className="max-w-24 truncate">{entry.name}</span>
                </button>
              </span>
            ))}
            {expanding && <Spinner size="xs" className="ml-1 text-zinc-400" />}
          </div>
        )}

        {/* Graph view */}
        <div className="relative flex-1">
          {expanding && !focusedNode && (
            <div className="absolute inset-0 z-10 flex items-center justify-center bg-zinc-50/80 dark:bg-zinc-950/80">
              <Spinner size="md" className="text-emerald-500" />
            </div>
          )}
          <ExploreGraphView
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
          <span className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
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
                    <span className="shrink-0 font-medium text-zinc-500 dark:text-zinc-400">
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
                <span className="text-[9px] text-zinc-400">
                  ID: {focusedNode.elementId}
                </span>
              </div>

              {/* Relationships section */}
              {groupedRelationships.length > 0 && (
                <div className="border-t border-zinc-100 pt-3 dark:border-zinc-800">
                  <span className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
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
                          <span className="font-mono font-medium text-zinc-600 dark:text-zinc-400">
                            {group.type}
                          </span>
                          <span className="text-zinc-400">
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
                              <span className="rounded bg-zinc-100 px-1 py-0.5 text-[8px] font-medium text-zinc-500 dark:bg-zinc-800 dark:text-zinc-400">
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
            <div className="flex h-full items-center justify-center text-xs text-zinc-400">
              Select an entity to view properties
            </div>
          )}
        </div>
      </div>
    </div>
    </ErrorBoundary>
  );
}
