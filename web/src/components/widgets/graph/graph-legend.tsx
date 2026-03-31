"use client";

import { memo } from "react";

// ---------------------------------------------------------------------------
// Legend — type-color mapping
// ---------------------------------------------------------------------------

interface LegendProps {
  typeColorIndex: Map<string, string>;
}

export const Legend = memo(function Legend({ typeColorIndex }: LegendProps) {
  if (typeColorIndex.size <= 1) return null;
  const entries = Array.from(typeColorIndex.entries());
  if (entries.length > 12) return null; // too many types, skip legend

  return (
    <div className="absolute bottom-2 left-2 z-10 flex flex-wrap gap-x-3 gap-y-1 rounded-md bg-white/90 px-2 py-1.5 text-[10px] shadow-sm backdrop-blur dark:bg-zinc-800/90">
      {entries.map(([type, color]) => (
        <div key={type} className="flex items-center gap-1">
          <span
            className="inline-block h-2.5 w-2.5 rounded-full"
            style={{ backgroundColor: color }}
          />
          <span className="text-zinc-600 dark:text-zinc-300">{type}</span>
        </div>
      ))}
    </div>
  );
});
