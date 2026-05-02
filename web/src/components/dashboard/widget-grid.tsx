"use client";

import React from "react";
import ReactGridLayout from "react-grid-layout";
import "react-grid-layout/css/styles.css";
import { WidgetCard } from "./widget-card";
import type { DashboardWidget } from "@/types/api";

/**
 * Build the runtime prop bag for `<ReactGridLayout>`. Hoisted outside
 * the component body so the single-site `any` cast lives on exactly
 * one line — see the JSX site for why the cast is necessary.
 */
function gridProps(args: {
  layout: Array<{ i: string; x: number; y: number; w: number; h: number; minW: number; minH: number }>;
  width: number;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
}): any {
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
      {/*
        Why `as any` on `gridProps`: @types/react-grid-layout@1.3.6 types
        `cols` for the Responsive variant only — the default export
        accepts the same prop at runtime but its `GridLayoutProps` type
        omits it. A d.ts augmentation or a fork of @types/... is the
        proper long-term fix; a hoisted prop bag with a single disable
        is the minimum-risk stop-gap for now.
      */}
      <ReactGridLayout
        {...(gridProps({ layout, width }))}
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
