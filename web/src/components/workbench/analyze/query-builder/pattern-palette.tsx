"use client";

import { useState, useCallback } from "react";
import type { NodeTypeDef, EdgeTypeDef } from "@/types/api";
import type { Suggestion } from "./use-suggestions";

// ---------------------------------------------------------------------------
// PatternPalette — Available node/edge types from ontology
// ---------------------------------------------------------------------------

export type PaletteTab = "nodes" | "edges" | "suggested";

interface PatternPaletteProps {
  nodeTypes: NodeTypeDef[];
  edgeTypes: EdgeTypeDef[];
  onAddNode: (nodeType: NodeTypeDef) => void;
  onAddEdge: (edgeType: EdgeTypeDef) => void;
  suggestions?: Suggestion[];
  selectedNodeLabel?: string | null;
  onAddSuggestion?: (suggestion: Suggestion) => void;
  activeTab?: PaletteTab;
  onTabChange?: (tab: PaletteTab) => void;
}

export function PatternPalette({
  nodeTypes,
  edgeTypes,
  onAddNode,
  onAddEdge,
  suggestions = [],
  selectedNodeLabel = null,
  onAddSuggestion,
  activeTab,
  onTabChange,
}: PatternPaletteProps) {
  const [search, setSearch] = useState("");
  const [internalTab, setInternalTab] = useState<PaletteTab>("nodes");

  const tab = activeTab ?? internalTab;
  const setTab = onTabChange ?? setInternalTab;

  const lowerSearch = search.toLowerCase();

  const filteredNodes = nodeTypes.filter(
    (nt) =>
      nt.label.toLowerCase().includes(lowerSearch) ||
      nt.description?.toLowerCase().includes(lowerSearch),
  );

  const filteredEdges = edgeTypes.filter(
    (et) =>
      et.label.toLowerCase().includes(lowerSearch) ||
      et.description?.toLowerCase().includes(lowerSearch),
  );

  const handleDragStartNode = useCallback(
    (e: React.DragEvent, nodeType: NodeTypeDef) => {
      e.dataTransfer.setData("application/qb-node", JSON.stringify(nodeType));
      e.dataTransfer.effectAllowed = "copy";
    },
    [],
  );

  const handleDragStartEdge = useCallback(
    (e: React.DragEvent, edgeType: EdgeTypeDef) => {
      e.dataTransfer.setData("application/qb-edge", JSON.stringify(edgeType));
      e.dataTransfer.effectAllowed = "copy";
    },
    [],
  );

  return (
    <div className="flex h-full flex-col">
      {/* Search */}
      <div className="shrink-0 p-2">
        <input
          type="text"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          placeholder="Search types..."
          className="h-7 w-full rounded border border-zinc-200 bg-white px-2 text-xs text-zinc-700 placeholder:text-zinc-400 dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-300"
        />
      </div>

      {/* Tabs */}
      <div className="flex shrink-0 border-b border-zinc-200 px-2 dark:border-zinc-800">
        <button
          onClick={() => setTab("nodes")}
          className={`px-3 py-1.5 text-xs font-medium transition-colors ${
            tab === "nodes"
              ? "border-b-2 border-emerald-600 text-emerald-600 dark:border-emerald-400 dark:text-emerald-400"
              : "text-zinc-500 hover:text-zinc-700 dark:text-zinc-400 dark:hover:text-zinc-300"
          }`}
        >
          Nodes ({filteredNodes.length})
        </button>
        <button
          onClick={() => setTab("edges")}
          className={`px-3 py-1.5 text-xs font-medium transition-colors ${
            tab === "edges"
              ? "border-b-2 border-emerald-600 text-emerald-600 dark:border-emerald-400 dark:text-emerald-400"
              : "text-zinc-500 hover:text-zinc-700 dark:text-zinc-400 dark:hover:text-zinc-300"
          }`}
        >
          Edges ({filteredEdges.length})
        </button>
        {selectedNodeLabel && (
          <button
            onClick={() => setTab("suggested")}
            className={`px-3 py-1.5 text-xs font-medium transition-colors ${
              tab === "suggested"
                ? "border-b-2 border-violet-600 text-violet-600 dark:border-violet-400 dark:text-violet-400"
                : "text-zinc-500 hover:text-zinc-700 dark:text-zinc-400 dark:hover:text-zinc-300"
            }`}
          >
            Suggested ({suggestions.length})
          </button>
        )}
      </div>

      {/* List */}
      <div className="flex-1 overflow-auto p-2 space-y-1">
        {tab === "nodes" &&
          filteredNodes.map((nt) => (
            <div
              key={nt.id}
              draggable
              onDragStart={(e) => handleDragStartNode(e, nt)}
              onClick={() => onAddNode(nt)}
              className="group cursor-grab rounded-lg border border-zinc-200 bg-white px-3 py-2 transition-colors hover:border-emerald-300 hover:bg-emerald-50/50 active:cursor-grabbing dark:border-zinc-700 dark:bg-zinc-800 dark:hover:border-emerald-700 dark:hover:bg-emerald-950/20"
            >
              <div className="flex items-center gap-2">
                <div className="h-2.5 w-2.5 shrink-0 rounded-full bg-blue-400 dark:bg-blue-500" />
                <span className="text-xs font-medium text-zinc-700 dark:text-zinc-300">
                  {nt.label}
                </span>
              </div>
              {nt.description && (
                <p className="mt-0.5 text-[10px] text-zinc-400 line-clamp-1">
                  {nt.description}
                </p>
              )}
              <div className="mt-1 text-[10px] text-zinc-400">
                {nt.properties.length} properties
              </div>
            </div>
          ))}

        {tab === "edges" &&
          filteredEdges.map((et) => {
            const srcLabel =
              nodeTypes.find((n) => n.id === et.source_node_id)?.label ?? "?";
            const tgtLabel =
              nodeTypes.find((n) => n.id === et.target_node_id)?.label ?? "?";
            return (
              <div
                key={et.id}
                draggable
                onDragStart={(e) => handleDragStartEdge(e, et)}
                onClick={() => onAddEdge(et)}
                className="group cursor-grab rounded-lg border border-zinc-200 bg-white px-3 py-2 transition-colors hover:border-emerald-300 hover:bg-emerald-50/50 active:cursor-grabbing dark:border-zinc-700 dark:bg-zinc-800 dark:hover:border-emerald-700 dark:hover:bg-emerald-950/20"
              >
                <div className="flex items-center gap-2">
                  <div className="h-2.5 w-2.5 shrink-0 rounded-sm bg-amber-400 dark:bg-amber-500" />
                  <span className="text-xs font-medium text-zinc-700 dark:text-zinc-300">
                    {et.label}
                  </span>
                </div>
                <p className="mt-0.5 text-[10px] text-zinc-400">
                  {srcLabel} &rarr; {tgtLabel}
                </p>
              </div>
            );
          })}

        {tab === "nodes" && filteredNodes.length === 0 && (
          <p className="py-4 text-center text-xs text-zinc-400">
            No node types found
          </p>
        )}
        {tab === "edges" && filteredEdges.length === 0 && (
          <p className="py-4 text-center text-xs text-zinc-400">
            No edge types found
          </p>
        )}

        {tab === "suggested" && !selectedNodeLabel && (
          <p className="py-4 text-center text-xs text-zinc-400">
            Select a node to see related edges
          </p>
        )}

        {tab === "suggested" &&
          selectedNodeLabel &&
          suggestions.map((s, i) => (
            <div
              key={`${s.edge.id}-${s.direction}-${i}`}
              onClick={() => onAddSuggestion?.(s)}
              className="group cursor-pointer rounded-lg border border-zinc-200 bg-white px-3 py-2 transition-colors hover:border-violet-300 hover:bg-violet-50/50 dark:border-zinc-700 dark:bg-zinc-800 dark:hover:border-violet-700 dark:hover:bg-violet-950/20"
            >
              <div className="flex items-center gap-2">
                <span className="shrink-0 text-[10px] text-zinc-400">
                  {s.direction === "outgoing" ? "\u2192" : "\u2190"}
                </span>
                <span className="text-xs font-medium text-zinc-700 dark:text-zinc-300">
                  {s.edge.label}
                </span>
                <span
                  className={`ml-auto h-2 w-2 shrink-0 rounded-full ${
                    s.alreadyInPattern
                      ? "bg-emerald-400 dark:bg-emerald-500"
                      : "bg-blue-400 dark:bg-blue-500"
                  }`}
                />
              </div>
              <p className="mt-0.5 text-[10px] text-zinc-400">
                {s.direction === "outgoing" ? "\u2192 " : "\u2190 "}
                {s.targetNode.label}
              </p>
            </div>
          ))}

        {tab === "suggested" &&
          selectedNodeLabel &&
          suggestions.length === 0 && (
            <p className="py-4 text-center text-xs text-zinc-400">
              No related edges found
            </p>
          )}
      </div>
    </div>
  );
}
