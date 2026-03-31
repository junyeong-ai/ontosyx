"use client";

import { useAppStore } from "@/lib/store";

// ---------------------------------------------------------------------------
// Version diff overlay bar -- shown when comparing revisions
// ---------------------------------------------------------------------------

export function VersionDiffBar() {
  const diffOverlay = useAppStore((s) => s.activeDiffOverlay);
  const setDiffOverlay = useAppStore((s) => s.setActiveDiffOverlay);

  if (!diffOverlay || !diffOverlay.summary.total_changes) return null;

  const { summary } = diffOverlay;

  return (
    <div className="absolute left-1/2 bottom-3 z-10 -translate-x-1/2">
      <div className="flex items-center gap-3 rounded-lg border border-purple-200 bg-purple-50/95 px-4 py-2 text-xs shadow-lg backdrop-blur-sm dark:border-purple-900 dark:bg-purple-950/95">
        <span className="font-semibold text-purple-700 dark:text-purple-300">
          Version diff
        </span>
        {summary.nodes_added > 0 && (
          <span className="text-emerald-600 dark:text-emerald-400">
            +{summary.nodes_added}N
          </span>
        )}
        {summary.nodes_removed > 0 && (
          <span className="text-red-600 dark:text-red-400">
            -{summary.nodes_removed}N
          </span>
        )}
        {summary.nodes_modified > 0 && (
          <span className="text-amber-600 dark:text-amber-400">
            ~{summary.nodes_modified}N
          </span>
        )}
        {summary.edges_added > 0 && (
          <span className="text-emerald-600 dark:text-emerald-400">
            +{summary.edges_added}E
          </span>
        )}
        {summary.edges_removed > 0 && (
          <span className="text-red-600 dark:text-red-400">
            -{summary.edges_removed}E
          </span>
        )}
        {summary.edges_modified > 0 && (
          <span className="text-amber-600 dark:text-amber-400">
            ~{summary.edges_modified}E
          </span>
        )}
        <button
          onClick={() => setDiffOverlay(null)}
          className="ml-1 rounded-md px-2 py-0.5 text-zinc-500 hover:bg-white/50 hover:text-zinc-700 dark:hover:bg-zinc-800/50"
        >
          Dismiss
        </button>
      </div>
    </div>
  );
}
