"use client";

import { useMemo, useState } from "react";
import { cn } from "@/lib/cn";
import type { QueryResult, WidgetSpec } from "@/types/api";

interface HeatmapWidgetProps {
  spec: WidgetSpec;
  data: QueryResult;
}

/** Interpolate between blue (low) → yellow (mid) → red (high) */
function heatColor(ratio: number): string {
  // ratio 0..1
  const r = ratio < 0.5 ? Math.round(ratio * 2 * 255) : 255;
  const g = ratio < 0.5 ? Math.round(ratio * 2 * 255) : Math.round((1 - ratio) * 2 * 255);
  const b = ratio < 0.5 ? Math.round(255 - ratio * 2 * 255) : 0;
  return `rgb(${r}, ${g}, ${b})`;
}

export function HeatmapWidget({ spec, data }: HeatmapWidgetProps) {
  const [hoveredCell, setHoveredCell] = useState<{
    x: string;
    y: string;
    value: number;
    left: number;
    top: number;
  } | null>(null);

  const { columns, rows } = data;

  // Resolve column names: x, y, value
  const xCol = useMemo(() => {
    if (spec.data_mapping?.label) return spec.data_mapping.label;
    const lower = columns.map((c) => c.toLowerCase());
    const idx = lower.findIndex((c) => c === "x" || c === "column" || c === "col");
    return idx >= 0 ? columns[idx] : columns[0];
  }, [spec, columns]);

  const yCol = useMemo(() => {
    const lower = columns.map((c) => c.toLowerCase());
    const idx = lower.findIndex((c) => c === "y" || c === "row");
    return idx >= 0 ? columns[idx] : columns[1];
  }, [columns]);

  const valueCol = useMemo(() => {
    if (spec.data_mapping?.value) return spec.data_mapping.value;
    const lower = columns.map((c) => c.toLowerCase());
    const idx = lower.findIndex((c) => c === "value" || c === "count" || c === "score");
    if (idx >= 0) return columns[idx];
    // Fallback: first numeric column that is not x or y
    if (rows.length > 0) {
      for (const col of columns) {
        if (col !== xCol && col !== yCol && typeof rows[0][col] === "number") return col;
      }
    }
    return columns[2];
  }, [spec, columns, rows, xCol, yCol]);

  // Build grid data
  const { xLabels, yLabels, grid, min, max } = useMemo(() => {
    const xs = new Set<string>();
    const ys = new Set<string>();
    const map = new Map<string, number>();

    for (const row of rows) {
      const x = String(row[xCol] ?? "");
      const y = String(row[yCol] ?? "");
      const v = Number(row[valueCol] ?? 0);
      xs.add(x);
      ys.add(y);
      map.set(`${x}::${y}`, v);
    }

    const xLabels = Array.from(xs);
    const yLabels = Array.from(ys);
    const values = Array.from(map.values());
    const min = Math.min(...values);
    const max = Math.max(...values);

    return { xLabels, yLabels, grid: map, min, max };
  }, [rows, xCol, yCol, valueCol]);

  if (!xCol || !yCol || !valueCol || columns.length < 3) {
    return <p className="text-xs text-zinc-400">Need x, y, and value columns for heatmap</p>;
  }

  const range = max - min || 1;

  return (
    <div className="space-y-2">
      {spec.title && (
        <h4 className="text-xs font-semibold text-zinc-600 dark:text-zinc-400">
          {spec.title}
        </h4>
      )}
      <div className="relative max-h-80 overflow-auto">
        {/* Column headers */}
        <div className="flex">
          <div className="w-16 shrink-0" />
          {xLabels.map((x) => (
            <div
              key={x}
              className="flex-1 min-w-[36px] px-0.5 text-center text-[10px] font-medium text-zinc-500 dark:text-zinc-400 truncate"
              title={x}
            >
              {x}
            </div>
          ))}
        </div>
        {/* Grid rows */}
        {yLabels.map((y) => (
          <div key={y} className="flex items-center">
            <div
              className="w-16 shrink-0 truncate pr-1 text-right text-[10px] font-medium text-zinc-500 dark:text-zinc-400"
              title={y}
            >
              {y}
            </div>
            {xLabels.map((x) => {
              const value = grid.get(`${x}::${y}`) ?? 0;
              const ratio = (value - min) / range;
              return (
                <div
                  key={`${x}::${y}`}
                  className="flex-1 min-w-[36px] aspect-square m-0.5 rounded-sm cursor-default transition-transform hover:scale-110"
                  style={{ backgroundColor: heatColor(ratio) }}
                  onMouseEnter={(e) => {
                    const rect = e.currentTarget.getBoundingClientRect();
                    setHoveredCell({
                      x,
                      y,
                      value,
                      left: rect.left + rect.width / 2,
                      top: rect.top,
                    });
                  }}
                  onMouseLeave={() => setHoveredCell(null)}
                />
              );
            })}
          </div>
        ))}
        {/* Tooltip */}
        {hoveredCell && (
          <div
            className={cn(
              "pointer-events-none fixed z-50 -translate-x-1/2 -translate-y-full",
              "rounded-md px-2 py-1 text-[10px] font-medium shadow-md",
              "bg-zinc-900 text-zinc-100 dark:bg-zinc-100 dark:text-zinc-900",
            )}
            style={{ left: hoveredCell.left, top: hoveredCell.top - 4 }}
          >
            {hoveredCell.x} / {hoveredCell.y}: {hoveredCell.value.toLocaleString()}
          </div>
        )}
      </div>
      {/* Legend */}
      <div className="flex items-center gap-2 text-[10px] text-zinc-400">
        <span>{min.toLocaleString()}</span>
        <div
          className="h-2 flex-1 rounded-full"
          style={{
            background: `linear-gradient(to right, ${heatColor(0)}, ${heatColor(0.5)}, ${heatColor(1)})`,
          }}
        />
        <span>{max.toLocaleString()}</span>
        <span className="ml-2">{rows.length} cells</span>
      </div>
    </div>
  );
}
