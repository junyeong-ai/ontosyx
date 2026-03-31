"use client";

import { memo } from "react";
import type { GraphNodeData } from "./graph-types";
import { formatValue } from "../chart-utils";

// ---------------------------------------------------------------------------
// NodeDetailPanel — shows selected node properties
// ---------------------------------------------------------------------------

interface NodeDetailPanelProps {
  node: GraphNodeData;
  onClose: () => void;
}

export const NodeDetailPanel = memo(function NodeDetailPanel({
  node,
  onClose,
}: NodeDetailPanelProps) {
  const entries = Object.entries(node.properties).filter(
    ([, v]) => v != null,
  );

  return (
    <div
      className="absolute right-2 top-2 z-10 w-64 overflow-hidden rounded-lg border border-zinc-200 bg-white shadow-lg dark:border-zinc-700 dark:bg-zinc-800"
      role="dialog"
      aria-label={`Properties of ${node.label}`}
    >
      <div className="flex items-center justify-between border-b border-zinc-100 px-3 py-2 dark:border-zinc-700">
        <div className="min-w-0">
          <div className="truncate text-xs font-semibold text-zinc-800 dark:text-zinc-100">
            {node.label}
          </div>
          {node.type && (
            <div className="truncate text-[10px] text-zinc-500 dark:text-zinc-400">
              {node.type}
            </div>
          )}
        </div>
        <button
          onClick={onClose}
          className="ml-2 flex h-5 w-5 shrink-0 items-center justify-center rounded text-zinc-400 transition-colors hover:bg-zinc-100 hover:text-zinc-600 dark:hover:bg-zinc-700 dark:hover:text-zinc-300"
          aria-label="Close detail panel"
        >
          <svg viewBox="0 0 12 12" className="h-3 w-3" fill="currentColor">
            <path d="M3.05 3.05a.5.5 0 01.7 0L6 5.29l2.25-2.24a.5.5 0 01.7.7L6.71 6l2.24 2.25a.5.5 0 01-.7.7L6 6.71 3.75 8.95a.5.5 0 01-.7-.7L5.29 6 3.05 3.75a.5.5 0 010-.7z" />
          </svg>
        </button>
      </div>
      <div className="max-h-48 overflow-auto px-3 py-2">
        {entries.length > 0 ? (
          <dl className="space-y-1">
            {entries.map(([key, val]) => (
              <div key={key} className="flex justify-between gap-2 text-[10px]">
                <dt className="shrink-0 font-medium text-zinc-500 dark:text-zinc-400">
                  {key}
                </dt>
                <dd className="truncate text-right text-zinc-700 dark:text-zinc-300">
                  {formatValue(val)}
                </dd>
              </div>
            ))}
          </dl>
        ) : (
          <p className="text-[10px] text-zinc-400">No properties</p>
        )}
      </div>
    </div>
  );
});
