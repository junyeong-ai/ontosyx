"use client";

import React from "react";
import ReactGridLayout from "react-grid-layout";
import "react-grid-layout/css/styles.css";
import { WidgetCard } from "./widget-card";
import type { DashboardWidget } from "@/types/api";

// `LayoutItem` mirrors the `Layout` interface from
// @types/react-grid-layout — the package's namespaced export pattern
// (`export = ReactGridLayout`) makes the named type imports awkward,
// so re-stating the small shape here is the cleanest way to keep
// the prop bag typed end-to-end.
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
  onLayoutChange: (layout: unknown[]) => void;
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
