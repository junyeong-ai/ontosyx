"use client";

import React from "react";
import ReactGridLayout from "react-grid-layout";
import "react-grid-layout/css/styles.css";
import { WidgetCard } from "./widget-card";
import type { DashboardLayoutItem, DashboardWidget } from "@/types/api";

// `LayoutItem` mirrors the runtime shape emitted by react-grid-layout.
// Keeping the local type narrow makes the dashboard contract independent
// from upstream prop-type details while preserving end-to-end typing.
//
// `GridLayoutProps` augments react-grid-layout's CoreProps with
// `cols` — typed for the Responsive variant only in the upstream
// types but accepted at runtime on the default export. The
// augmentation removes the JSX-boundary cast.
interface LayoutItem {
  i: string;
  x: number;
  y: number;
  w: number;
  h: number;
  minW?: number;
  minH?: number;
}
interface GridLayoutProps {
  className?: string;
  layout: LayoutItem[];
  cols: number;
  rowHeight: number;
  width: number;
  isDraggable?: boolean;
  isResizable?: boolean;
}

function gridProps(args: {
  layout: LayoutItem[];
  width: number;
}): GridLayoutProps {
  return {
    className: "layout",
    layout: args.layout,
    cols: 12,
    rowHeight: 60,
    width: args.width,
    isDraggable: true,
    isResizable: true,
  };
}

export interface WidgetGridProps {
  widgets: DashboardWidget[];
  selectedWidgetId: string | null;
  refreshKey?: number;
  onSelect: (id: string) => void;
  onLayoutChange: (layout: DashboardLayoutItem[]) => void;
}

export function WidgetGrid({
  widgets,
  selectedWidgetId,
  refreshKey,
  onSelect,
  onLayoutChange,
}: WidgetGridProps) {
  const layout: LayoutItem[] = widgets.map((w) => {
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
      <ReactGridLayout
        {...gridProps({ layout, width })}
        onLayoutChange={(newLayout) => {
          const items = Array.isArray(newLayout) ? newLayout : [newLayout];
          onLayoutChange(
            items.map((item: { i: string; x: number; y: number; w: number; h: number }) => ({
              widget_id: item.i,
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
