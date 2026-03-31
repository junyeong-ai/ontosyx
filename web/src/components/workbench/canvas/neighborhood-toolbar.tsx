"use client";

import { useAppStore } from "@/lib/store";

export function NeighborhoodToolbar() {
  const neighborhoodFocus = useAppStore((s) => s.neighborhoodFocus);
  const setNeighborhoodFocus = useAppStore((s) => s.setNeighborhoodFocus);

  if (!neighborhoodFocus) return null;

  const { nodeId, depth } = neighborhoodFocus;

  return (
    <div className="absolute top-3 left-1/2 z-10 flex -translate-x-1/2 items-center gap-1 rounded-lg border border-zinc-200 bg-white px-2 py-1 shadow-md dark:border-zinc-700 dark:bg-zinc-900">
      <span className="mr-2 text-xs text-zinc-500">Neighborhood</span>
      {([1, 2, 3] as const).map((d) => (
        <button
          key={d}
          onClick={() => setNeighborhoodFocus({ nodeId, depth: d })}
          className={`rounded px-2 py-0.5 text-xs font-medium transition-colors ${
            depth === d
              ? "bg-emerald-100 text-emerald-700 dark:bg-emerald-900 dark:text-emerald-300"
              : "text-zinc-500 hover:bg-zinc-100 dark:hover:bg-zinc-800"
          }`}
        >
          {d}-hop
        </button>
      ))}
      <button
        onClick={() => setNeighborhoodFocus(null)}
        className="ml-1 rounded px-2 py-0.5 text-xs font-medium text-zinc-500 hover:bg-zinc-100 dark:hover:bg-zinc-800"
      >
        All
      </button>
      <span className="ml-1 text-[10px] text-zinc-400">(Esc to exit)</span>
    </div>
  );
}
