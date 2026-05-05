"use client";

import { Separator } from "react-resizable-panels";
import { cn } from "@/lib/cn";

interface ResizeHandleProps {
  orientation?: "horizontal" | "vertical";
}

export function ResizeHandle({ orientation = "horizontal" }: ResizeHandleProps) {
  const isVertical = orientation === "vertical";
  return (
    <Separator
      className={cn(
        "group relative flex items-center justify-center transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)]",
        "hover:bg-brand-surface",
        isVertical ? "h-2 cursor-row-resize" : "w-2 cursor-col-resize",
      )}
    >
      <div
        className={cn(
          "flex items-center justify-center gap-px",
          isVertical ? "flex-row" : "flex-col",
        )}
      >
        <div className="h-1 w-1 rounded-full bg-divider transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] group-hover:bg-brand-foreground" />
        <div className="h-1 w-1 rounded-full bg-divider transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] group-hover:bg-brand-foreground" />
        <div className="h-1 w-1 rounded-full bg-divider transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] group-hover:bg-brand-foreground" />
      </div>
    </Separator>
  );
}
