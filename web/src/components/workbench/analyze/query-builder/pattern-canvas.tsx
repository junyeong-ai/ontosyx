"use client";

import { useCallback } from "react";
import type { NodeTypeDef, EdgeTypeDef } from "@/types/api";
import type { PatternNode, PatternEdge } from "./ir-builder";

// ---------------------------------------------------------------------------
// PatternCanvas — Visual pattern builder (div-based, not full XyFlow)
// ---------------------------------------------------------------------------

interface PatternCanvasProps {
  nodes: PatternNode[];
  edges: PatternEdge[];
  nodeTypes: NodeTypeDef[];
  edgeTypes: EdgeTypeDef[];
  selectedId: string | null;
  onSelectNode: (nodeId: string | null) => void;
  onSelectEdge: (edgeId: string | null) => void;
  onAddNode: (nodeType: NodeTypeDef) => void;
  onAddEdge: (edgeType: EdgeTypeDef) => void;
  onRemoveNode: (nodeId: string) => void;
  onRemoveEdge: (edgeId: string) => void;
}

export function PatternCanvas({
  nodes,
  edges,
  nodeTypes,
  selectedId,
  onSelectNode,
  onSelectEdge,
  onAddNode,
  onAddEdge,
  onRemoveNode,
  onRemoveEdge,
}: PatternCanvasProps) {
  const handleDrop = useCallback(
    (e: React.DragEvent) => {
      e.preventDefault();
      const nodeData = e.dataTransfer.getData("application/qb-node");
      const edgeData = e.dataTransfer.getData("application/qb-edge");
      if (nodeData) {
        try {
          const nt = JSON.parse(nodeData) as NodeTypeDef;
          onAddNode(nt);
        } catch { /* ignore parse errors */ }
      } else if (edgeData) {
        try {
          const et = JSON.parse(edgeData) as EdgeTypeDef;
          onAddEdge(et);
        } catch { /* ignore parse errors */ }
      }
    },
    [onAddNode, onAddEdge],
  );

  const handleDragOver = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    e.dataTransfer.dropEffect = "copy";
  }, []);

  const handleCanvasClick = useCallback(
    (e: React.MouseEvent) => {
      if (e.target === e.currentTarget) {
        onSelectNode(null);
        onSelectEdge(null);
      }
    },
    [onSelectNode, onSelectEdge],
  );

  if (nodes.length === 0) {
    return (
      <div
        onDrop={handleDrop}
        onDragOver={handleDragOver}
        className="flex h-full flex-col items-center justify-center gap-3 rounded-lg border-2 border-dashed border-zinc-200 p-8 text-center dark:border-zinc-700"
      >
        <div className="flex h-12 w-12 items-center justify-center rounded-full bg-zinc-100 dark:bg-zinc-800">
          <svg className="h-5 w-5 text-zinc-400" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
            <path d="M12 4.5v15m7.5-7.5h-15" />
          </svg>
        </div>
        <p className="text-sm font-medium text-zinc-600 dark:text-zinc-400">
          Build your query pattern
        </p>
        <p className="text-xs text-zinc-400">
          Drag node or edge types from the palette, or click them to add.
        </p>
      </div>
    );
  }

  // Track which edges have been rendered so each badge appears only once
  const renderedEdgeIds = new Set<string>();

  return (
    <div
      onDrop={handleDrop}
      onDragOver={handleDragOver}
      onClick={handleCanvasClick}
      className="relative h-full overflow-auto rounded-lg border border-zinc-200 bg-zinc-50/50 p-4 dark:border-zinc-700 dark:bg-zinc-900/50"
    >
      {/* Node cards — flow layout */}
      <div className="flex flex-wrap items-start gap-3">
        {nodes.map((node, idx) => {
          const isSelected = selectedId === node.id;
          const nt = nodeTypes.find((t) => t.label === node.label);
          const propCount = nt?.properties.length ?? 0;

          // Find incoming edges not yet rendered
          const incomingEdges = edges.filter(
            (e) => e.targetNodeId === node.id && !renderedEdgeIds.has(e.id),
          );
          // Mark them as rendered
          incomingEdges.forEach((e) => renderedEdgeIds.add(e.id));

          return (
            <div key={node.id} className="flex items-center gap-2">
              {/* Edge arrows leading into this node */}
              {idx > 0 && incomingEdges.length > 0 && (
                <div className="flex flex-col items-center gap-0.5">
                  {incomingEdges.map((edge) => (
                      <div
                        key={edge.id}
                        role="button"
                        tabIndex={0}
                        onClick={(e) => {
                          e.stopPropagation();
                          onSelectEdge(edge.id);
                        }}
                        onKeyDown={(e) => { if (e.key === 'Enter') onSelectEdge(edge.id); }}
                        className={`group/edge relative flex cursor-pointer items-center gap-1 rounded-full border px-2 py-0.5 text-[10px] font-medium transition-colors ${
                          selectedId === edge.id
                            ? "border-amber-400 bg-amber-50 text-amber-700 dark:border-amber-600 dark:bg-amber-950/30 dark:text-amber-400"
                            : "border-zinc-300 bg-white text-zinc-500 hover:border-amber-300 dark:border-zinc-600 dark:bg-zinc-800 dark:text-zinc-400"
                        }`}
                      >
                        <span>&rarr;</span>
                        <span>{edge.relType}</span>
                        <button
                          onClick={(e) => {
                            e.stopPropagation();
                            onRemoveEdge(edge.id);
                          }}
                          className="ml-0.5 hidden text-zinc-400 hover:text-red-500 group-hover/edge:inline dark:hover:text-red-400"
                          title="Remove edge"
                        >
                          &times;
                        </button>
                      </div>
                    ))}
                </div>
              )}

              {/* Node card */}
              <div
                role="button"
                tabIndex={0}
                onClick={(e) => {
                  e.stopPropagation();
                  onSelectNode(isSelected ? null : node.id);
                }}
                onKeyDown={(e) => { if (e.key === 'Enter') onSelectNode(isSelected ? null : node.id); }}
                className={`group/node relative cursor-pointer rounded-xl border-2 px-4 py-3 text-left transition-all ${
                  isSelected
                    ? "border-emerald-500 bg-emerald-50 shadow-sm dark:border-emerald-600 dark:bg-emerald-950/30"
                    : "border-zinc-200 bg-white hover:border-emerald-300 hover:shadow-sm dark:border-zinc-700 dark:bg-zinc-800 dark:hover:border-emerald-700"
                }`}
              >
                {/* Remove button */}
                <button
                  onClick={(e) => {
                    e.stopPropagation();
                    onRemoveNode(node.id);
                  }}
                  className="absolute -right-1.5 -top-1.5 hidden h-5 w-5 items-center justify-center rounded-full bg-red-500 text-[10px] text-white shadow-sm group-hover/node:flex"
                  title="Remove node"
                >
                  &times;
                </button>

                <div className="flex items-center gap-2">
                  <div className="h-3 w-3 shrink-0 rounded-full bg-blue-400 dark:bg-blue-500" />
                  <span className="text-xs font-semibold text-zinc-800 dark:text-zinc-200">
                    {node.label}
                  </span>
                </div>

                <div className="mt-1 text-[10px] text-zinc-400">
                  {node.alias} &middot; {propCount} props
                  {node.filters.length > 0 && (
                    <span className="ml-1 text-amber-500">
                      &middot; {node.filters.length} filter{node.filters.length > 1 ? "s" : ""}
                    </span>
                  )}
                  {node.returnProps.length > 0 && (
                    <span className="ml-1 text-emerald-500">
                      &middot; {node.returnProps.length} return
                    </span>
                  )}
                </div>
              </div>
            </div>
          );
        })}
      </div>

      {/* Standalone edges (no specific layout connection) */}
      {edges.filter((e) => {
        const hasSource = nodes.some((n) => n.id === e.sourceNodeId);
        const hasTarget = nodes.some((n) => n.id === e.targetNodeId);
        return !hasSource || !hasTarget;
      }).length > 0 && (
        <div className="mt-4 flex flex-wrap gap-2">
          <span className="text-[10px] text-zinc-400">Unconnected edges:</span>
          {edges
            .filter((e) => {
              const hasSource = nodes.some((n) => n.id === e.sourceNodeId);
              const hasTarget = nodes.some((n) => n.id === e.targetNodeId);
              return !hasSource || !hasTarget;
            })
            .map((edge) => (
              <span
                key={edge.id}
                className="rounded-full border border-amber-200 bg-amber-50 px-2 py-0.5 text-[10px] text-amber-600 dark:border-amber-800 dark:bg-amber-950/30 dark:text-amber-400"
              >
                {edge.relType}
              </span>
            ))}
        </div>
      )}
    </div>
  );
}
