"use client";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { HugeiconsIcon } from "@hugeicons/react";
import { Search01Icon, Cancel01Icon } from "@hugeicons/core-free-icons";
import { searchGraph } from "@/lib/api";
import { Spinner } from "@/components/ui/spinner";
import { cn } from "@/lib/cn";
import { useAppStore } from "@/lib/store";
import type { NodeTypeDef, EdgeTypeDef } from "@/types/api";
import {
  type SearchResultNode,
  toSearchResultNodes,
  resolveDisplayName,
  resolveSubtitle,
} from "./explore/graph-utils";

// ---------------------------------------------------------------------------
// SearchDialog — Cmd+K graph entity search overlay
// ---------------------------------------------------------------------------

// --- Schema match types ---

interface SchemaNodeMatch {
  kind: "node";
  node: NodeTypeDef;
  matchReason: string;
}

interface SchemaEdgeMatch {
  kind: "edge";
  edge: EdgeTypeDef;
  sourceLabel: string;
  targetLabel: string;
  matchReason: string;
}

type SchemaMatch = SchemaNodeMatch | SchemaEdgeMatch;

// --- Schema search (local, instant) ---

function searchSchema(query: string, ontology: { node_types: NodeTypeDef[]; edge_types: EdgeTypeDef[] }): SchemaMatch[] {
  const q = query.toLowerCase();
  if (!q) return [];

  const nodeLabelMap = new Map<string, string>();
  for (const n of ontology.node_types) {
    nodeLabelMap.set(n.id, n.label);
  }

  const results: SchemaMatch[] = [];

  for (const node of ontology.node_types) {
    if (node.label.toLowerCase().includes(q)) {
      results.push({ kind: "node", node, matchReason: "label" });
    } else if (node.properties.some((p) => p.name.toLowerCase().includes(q))) {
      const matchingProp = node.properties.find((p) => p.name.toLowerCase().includes(q));
      results.push({ kind: "node", node, matchReason: `property: ${matchingProp?.name}` });
    } else if (node.description?.toLowerCase().includes(q)) {
      results.push({ kind: "node", node, matchReason: "description" });
    }
  }

  for (const edge of ontology.edge_types) {
    const sourceLabel = nodeLabelMap.get(edge.source_node_id) ?? "?";
    const targetLabel = nodeLabelMap.get(edge.target_node_id) ?? "?";
    if (edge.label.toLowerCase().includes(q)) {
      results.push({ kind: "edge", edge, sourceLabel, targetLabel, matchReason: "label" });
    } else if (sourceLabel.toLowerCase().includes(q) || targetLabel.toLowerCase().includes(q)) {
      results.push({ kind: "edge", edge, sourceLabel, targetLabel, matchReason: "endpoint" });
    } else if (edge.properties.some((p) => p.name.toLowerCase().includes(q))) {
      const matchingProp = edge.properties.find((p) => p.name.toLowerCase().includes(q));
      results.push({ kind: "edge", edge, sourceLabel, targetLabel, matchReason: `property: ${matchingProp?.name}` });
    }
  }

  return results;
}

export function SearchDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  const inputRef = useRef<HTMLInputElement>(null);
  const [query, setQuery] = useState("");
  const [dataHits, setDataHits] = useState<SearchResultNode[]>([]);
  const [loading, setLoading] = useState(false);
  const [dataSearched, setDataSearched] = useState(false);
  const [selectedIdx, setSelectedIdx] = useState(0);

  const select = useAppStore((s) => s.select);
  const ontology = useAppStore((s) => s.ontology);

  // Instant schema search as user types
  const schemaMatches = useMemo(() => {
    if (!ontology || !query.trim()) return [];
    return searchSchema(query.trim(), ontology);
  }, [query, ontology]);

  // Total selectable items count
  const totalItems = schemaMatches.length + dataHits.length;

  // Focus input when opened
  useEffect(() => {
    if (open) {
      setQuery("");
      setDataHits([]);
      setDataSearched(false);
      setSelectedIdx(0);
      setTimeout(() => inputRef.current?.focus(), 0);
    }
  }, [open]);

  // Reset selected index when results change
  useEffect(() => {
    setSelectedIdx(0);
  }, [schemaMatches.length, dataHits.length]);

  const runDataSearch = useCallback(async (q: string) => {
    if (!q.trim()) return;
    setLoading(true);
    setDataSearched(true);
    try {
      const result = await searchGraph(q.trim());
      setDataHits(toSearchResultNodes(result));
    } catch {
      setDataHits([]);
    } finally {
      setLoading(false);
    }
  }, []);

  const handleSelectSchema = useCallback((match: SchemaMatch) => {
    if (match.kind === "node") {
      select({ type: "node", nodeId: match.node.id });
    } else {
      select({ type: "edge", edgeId: match.edge.id });
    }
    if (!useAppStore.getState().isInspectorOpen) {
      useAppStore.getState().toggleInspector();
    }
    onClose();
  }, [select, onClose]);

  const handleSelectData = useCallback((hit: SearchResultNode) => {
    // Find matching ontology node by label and select it on canvas
    const ont = useAppStore.getState().ontology;
    if (ont) {
      const matchLabel = hit.labels[0];
      const node = ont.node_types.find((n) => n.label === matchLabel);
      if (node) {
        select({ type: "node", nodeId: node.id });
        if (!useAppStore.getState().isInspectorOpen) {
          useAppStore.getState().toggleInspector();
        }
      }
    }
    onClose();
  }, [select, onClose]);

  const handleSelectByIndex = useCallback((idx: number) => {
    if (idx < schemaMatches.length) {
      handleSelectSchema(schemaMatches[idx]);
    } else {
      const dataIdx = idx - schemaMatches.length;
      if (dataIdx < dataHits.length) {
        handleSelectData(dataHits[dataIdx]);
      }
    }
  }, [schemaMatches, dataHits, handleSelectSchema, handleSelectData]);

  if (!open) return null;

  const hasQuery = query.trim().length > 0;
  const hasSchemaResults = schemaMatches.length > 0;
  const hasDataResults = dataHits.length > 0;

  return (
    <div className="fixed inset-0 z-50 flex items-start justify-center pt-[15vh]" onClick={onClose}>
      <div className="absolute inset-0 bg-black/20 dark:bg-black/40" />
      <div
        className="relative w-full max-w-lg rounded-xl border border-zinc-200 bg-white shadow-2xl dark:border-zinc-700 dark:bg-zinc-900"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Search input */}
        <div className="flex items-center gap-2 border-b border-zinc-200 px-3 py-2.5 dark:border-zinc-700">
          <HugeiconsIcon icon={Search01Icon} className="h-4 w-4 text-zinc-400" size="100%" />
          <input
            ref={inputRef}
            type="text"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Escape") {
                onClose();
              } else if (e.key === "ArrowDown") {
                e.preventDefault();
                setSelectedIdx((i) => Math.min(i + 1, totalItems - 1));
              } else if (e.key === "ArrowUp") {
                e.preventDefault();
                setSelectedIdx((i) => Math.max(i - 1, 0));
              } else if (e.key === "Enter" && !loading) {
                e.preventDefault();
                if (totalItems > 0) {
                  handleSelectByIndex(selectedIdx);
                } else if (!dataSearched) {
                  runDataSearch(query);
                }
              }
            }}
            placeholder="Search schema and data..."
            className="flex-1 bg-transparent text-sm text-zinc-800 outline-none placeholder:text-zinc-500 dark:text-zinc-200"
          />
          {loading && <Spinner size="xs" className="text-zinc-400" />}
          <button onClick={onClose} className="text-zinc-400 hover:text-zinc-600 dark:hover:text-zinc-300">
            <HugeiconsIcon icon={Cancel01Icon} className="h-3.5 w-3.5" size="100%" />
          </button>
        </div>

        {/* Results */}
        <div className="max-h-80 overflow-auto">
          {/* Empty state */}
          {!hasQuery && (
            <p className="px-4 py-6 text-center text-xs text-zinc-400">
              Type to search schema — press Enter to search data
            </p>
          )}

          {/* Typing hint: has query, no results yet */}
          {hasQuery && !hasSchemaResults && !dataSearched && !loading && (
            <p className="px-4 py-6 text-center text-xs text-zinc-400">
              No schema matches — press Enter to search data
            </p>
          )}

          {/* Schema results section */}
          {hasSchemaResults && (
            <div>
              <div className="px-3 py-1.5 text-[10px] font-semibold uppercase tracking-wider text-zinc-400">
                Schema Matches
              </div>
              {schemaMatches.map((match, i) => {
                const isSelected = i === selectedIdx;
                if (match.kind === "node") {
                  const propCount = match.node.properties.length;
                  const constraintCount = match.node.constraints?.length ?? 0;
                  return (
                    <button
                      key={`schema-node-${match.node.id}`}
                      onClick={() => handleSelectSchema(match)}
                      onMouseEnter={() => setSelectedIdx(i)}
                      className={cn(
                        "flex w-full items-center gap-2 px-4 py-1.5 text-left transition-colors",
                        isSelected
                          ? "bg-emerald-50 dark:bg-emerald-950/30"
                          : "hover:bg-zinc-50 dark:hover:bg-zinc-800",
                      )}
                    >
                      <span className="rounded bg-emerald-100 px-1.5 py-0.5 text-[10px] font-medium text-emerald-700 dark:bg-emerald-900 dark:text-emerald-400">
                        Node
                      </span>
                      <span className="flex-1 truncate text-xs font-medium text-zinc-800 dark:text-zinc-200">
                        {match.node.label}
                      </span>
                      <span className="text-[10px] text-zinc-400">
                        {propCount} prop{propCount !== 1 ? "s" : ""}
                        {constraintCount > 0 && `, ${constraintCount} constraint${constraintCount !== 1 ? "s" : ""}`}
                      </span>
                    </button>
                  );
                } else {
                  return (
                    <button
                      key={`schema-edge-${match.edge.id}`}
                      onClick={() => handleSelectSchema(match)}
                      onMouseEnter={() => setSelectedIdx(i)}
                      className={cn(
                        "flex w-full items-center gap-2 px-4 py-1.5 text-left transition-colors",
                        isSelected
                          ? "bg-emerald-50 dark:bg-emerald-950/30"
                          : "hover:bg-zinc-50 dark:hover:bg-zinc-800",
                      )}
                    >
                      <span className="rounded bg-sky-100 px-1.5 py-0.5 text-[10px] font-medium text-sky-700 dark:bg-sky-900 dark:text-sky-400">
                        Edge
                      </span>
                      <span className="flex-1 truncate text-xs text-zinc-800 dark:text-zinc-200">
                        <span className="text-zinc-400">{match.sourceLabel}</span>
                        {" → "}
                        <span className="font-medium">{match.edge.label}</span>
                        {" → "}
                        <span className="text-zinc-400">{match.targetLabel}</span>
                      </span>
                    </button>
                  );
                }
              })}
            </div>
          )}

          {/* Data search hint — shown when schema results exist but data not yet searched */}
          {hasQuery && hasSchemaResults && !dataSearched && !loading && (
            <div className="border-t border-zinc-100 px-4 py-2 text-center text-[10px] text-zinc-400 dark:border-zinc-800">
              Press Enter to also search Neo4j data
            </div>
          )}

          {/* Data results section */}
          {dataSearched && (
            <div className={cn(hasSchemaResults && "border-t border-zinc-100 dark:border-zinc-800")}>
              <div className="px-3 py-1.5 text-[10px] font-semibold uppercase tracking-wider text-zinc-400">
                Data Matches
                {loading && <Spinner size="xs" className="ml-1 inline-block text-zinc-400" />}
              </div>
              {!loading && !hasDataResults && (
                <p className="px-4 py-3 text-center text-[10px] text-zinc-400">
                  No data results for &ldquo;{query}&rdquo;
                </p>
              )}
              {dataHits.map((hit, i) => {
                const globalIdx = schemaMatches.length + i;
                const isSelected = globalIdx === selectedIdx;
                return (
                  <button
                    key={hit.elementId || `data-${i}`}
                    onClick={() => handleSelectData(hit)}
                    onMouseEnter={() => setSelectedIdx(globalIdx)}
                    className={cn(
                      "flex w-full items-start gap-3 px-4 py-2 text-left transition-colors",
                      isSelected
                        ? "bg-emerald-50 dark:bg-emerald-950/30"
                        : "hover:bg-zinc-50 dark:hover:bg-zinc-800",
                    )}
                  >
                    <div className="flex flex-wrap gap-1 pt-0.5">
                      {hit.labels.map((l) => (
                        <span
                          key={l}
                          className="rounded bg-zinc-100 px-1.5 py-0.5 text-[10px] font-medium text-zinc-600 dark:bg-zinc-800 dark:text-zinc-400"
                        >
                          {l}
                        </span>
                      ))}
                    </div>
                    <div className="min-w-0 flex-1">
                      <p className="truncate text-xs font-medium text-zinc-800 dark:text-zinc-200">
                        {resolveDisplayName(hit.props)}
                      </p>
                      <p className="truncate text-[10px] text-zinc-400">
                        {resolveSubtitle(hit.props)}
                      </p>
                    </div>
                  </button>
                );
              })}
            </div>
          )}
        </div>

        {/* Footer hint */}
        <div className="flex items-center gap-3 border-t border-zinc-200 px-3 py-1.5 text-[10px] text-zinc-400 dark:border-zinc-700">
          <span><kbd className="rounded border border-zinc-300 px-1 dark:border-zinc-600">Enter</kbd> search data</span>
          <span><kbd className="rounded border border-zinc-300 px-1 dark:border-zinc-600">&uarr;&darr;</kbd> navigate</span>
          <span><kbd className="rounded border border-zinc-300 px-1 dark:border-zinc-600">Esc</kbd> close</span>
          <span className="ml-auto text-zinc-300 dark:text-zinc-600">Schema results appear as you type</span>
        </div>
      </div>
    </div>
  );
}
