"use client";

import React from "react";
import ReactGridLayout from "react-grid-layout";
import "react-grid-layout/css/styles.css";
import { WidgetCard } from "./widget-card";
import type { DashboardWidget } from "@/types/api";

export interface WidgetGridProps {
  widgets: DashboardWidget[];
  selectedWidgetId: string | null;
  refreshKey?: number;
  onSelect: (id: string) => void;
  onLayoutChange: (layout: unknown[]) => void;
}

export function WidgetGrid({
  widgets,
  selectedWidgetId,
  refreshKey,
  onSelect,
  onLayoutChange,
}: WidgetGridProps) {
  const layout = widgets.map((w) => {
    const pos = w.position as { x?: number; y?: number; w?: number; h?: number } | undefined;
    return {
      i: w.id,
      x: pos?.x ?? 0,
      y: pos?.y ?? 0,
      w: pos?.w ?? 6,
      h: pos?.h ?? 4,
      minW: 2,
      minH: 2,
    };
  });

  const containerRef = React.useRef<HTMLDivElement>(null);
  const [width, setWidth] = React.useState(800);

  React.useEffect(() => {
    if (!containerRef.current) return;
    const observer = new ResizeObserver((entries) => {
      for (const entry of entries) {
        setWidth(entry.contentRect.width);
      }
    });
    observer.observe(containerRef.current);
    return () => observer.disconnect();
  }, []);

  return (
    <div ref={containerRef}>
      {/* eslint-disable-next-line @typescript-eslint/no-explicit-any */}
      <ReactGridLayout
        {...({
          className: "layout",
          layout,
          cols: 12,
          rowHeight: 60,
          width,
          isDraggable: true,
          isResizable: true,
        } as any)}
        onLayoutChange={(newLayout) => {
          const items = Array.isArray(newLayout) ? newLayout : [newLayout];
          onLayoutChange(
            items.map((item: { i: string; x: number; y: number; w: number; h: number }) => ({
              i: item.i,
              x: item.x,
              y: item.y,
              w: item.w,
              h: item.h,
            })),
          );
        }}
      >
        {widgets.map((w) => (
          <div key={w.id}>
            <WidgetCard
              widget={w}
              selected={w.id === selectedWidgetId}
              refreshKey={refreshKey}
              onClick={() => onSelect(w.id)}
            />
          </div>
        ))}
      </ReactGridLayout>
    </div>
  );
}
