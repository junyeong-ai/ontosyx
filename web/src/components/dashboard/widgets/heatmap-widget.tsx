"use client";

import { useMemo, useState } from "react";
import { useTranslations } from "next-intl";
import { cn } from "@/lib/cn";
import type { QueryResult, WidgetSpec } from "@/types/api";
import { useFormatters } from "@/hooks/use-formatters";

import { Heading } from "@/components/ui/heading";
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
  const t = useTranslations("widget.heatmap");
  const fmt = useFormatters();
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
    return <p className="text-xs text-foreground-muted">{t("needColumns")}</p>;
  }

  const range = max - min || 1;

  return (
    <div className="space-y-2">
      {spec.title && (
        <Heading level={4} size={6}>
          {spec.title}
        </Heading>
      )}
      <div className="relative max-h-80 overflow-auto">
        {/* Column headers */}
        <div className="flex">
          <div className="w-16 shrink-0" />
          {xLabels.map((x) => (
            <div
              key={x}
              className="flex-1 min-w-[36px] px-0.5 text-center text-2xs font-medium text-foreground-muted truncate"
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
              className="w-16 shrink-0 truncate pe-1 text-end text-2xs font-medium text-foreground-muted"
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
                  className="flex-1 min-w-[36px] aspect-square m-0.5 rounded-sm cursor-default transition-transform duration-[var(--duration-quick)] ease-[var(--ease-out)] hover:scale-110"
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
              "pointer-events-none fixed z-tooltip -translate-x-1/2 -translate-y-full",
              "rounded-md px-2 py-1 text-2xs font-medium shadow-2",
              "bg-surface-base text-foreground-strong",
            )}
            style={{ left: hoveredCell.left, top: hoveredCell.top - 4 }}
          >
            {hoveredCell.x} / {hoveredCell.y}: {fmt.number(hoveredCell.value)}
          </div>
        )}
      </div>
      {/* Legend */}
      <div className="flex items-center gap-2 text-2xs text-foreground-muted">
        <span>{fmt.number(min)}</span>
        <div
          className="h-2 flex-1 rounded-full"
          style={{
            background: `linear-gradient(to right, ${heatColor(0)}, ${heatColor(0.5)}, ${heatColor(1)})`,
          }}
        />
        <span>{fmt.number(max)}</span>
        <span className="ms-2">{t("cellsCount", { count: rows.length })}</span>
      </div>
    </div>
  );
}
